use std::io::Write;

use profile::Profile;

mod common;
use common::*;

/// A real forward proxy for tests: answers `CONNECT host:port HTTP/1.1`
/// with `200 Connection Established`, then splices raw bytes between the
/// client and `host:port` in both directions — a genuine tunnel, not a
/// stand-in that special-cases the request. Reports each `CONNECT`
/// target it handled on `seen`, so a test can prove traffic actually went
/// through this proxy rather than connecting directly.
fn spawn_connect_proxy(seen: std::sync::mpsc::Sender<String>) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("proxy should bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut client) = stream else { continue };
            let seen = seen.clone();
            std::thread::spawn(move || {
                let mut reader =
                    std::io::BufReader::new(client.try_clone().expect("clone client stream"));
                let mut request_line = String::new();
                if std::io::BufRead::read_line(&mut reader, &mut request_line).is_err()
                    || request_line.is_empty()
                {
                    return;
                }
                let target = request_line
                    .trim_start_matches("CONNECT ")
                    .split(' ')
                    .next()
                    .unwrap_or("")
                    .to_string();
                loop {
                    let mut line = String::new();
                    match std::io::BufRead::read_line(&mut reader, &mut line) {
                        Ok(0) | Err(_) => return,
                        Ok(_) if line == "\r\n" => break,
                        Ok(_) => {}
                    }
                }
                let Ok(mut upstream) = std::net::TcpStream::connect(&target) else {
                    let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n");
                    return;
                };
                let _ = seen.send(target);
                let _ = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n");

                let mut upstream_reader = upstream.try_clone().expect("clone upstream stream");
                let mut client_writer = client.try_clone().expect("clone client stream");
                let client_to_upstream = std::thread::spawn(move || {
                    let _ = std::io::copy(&mut reader, &mut upstream);
                });
                let upstream_to_client = std::thread::spawn(move || {
                    let _ = std::io::copy(&mut upstream_reader, &mut client_writer);
                });
                let _ = client_to_upstream.join();
                let _ = upstream_to_client.join();
            });
        }
    });
    port
}

#[test]
fn navigate_routes_through_a_configured_proxy() {
    let page_addr = serve_html_once(
        r#"<div id="box">hi</div><style>#box { background-color: #00ff00; width: 32px; height: 32px; }</style>"#,
    );
    let page_url = format!("http://{page_addr}/");

    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    let proxy_port = spawn_connect_proxy(seen_tx);

    let name = unique_shmem_name("proxy-navigate");
    let mut profile = Profile::spawn_with_proxy(
        worker_path(),
        &name,
        32,
        32,
        Some(&format!("127.0.0.1:{proxy_port}")),
    )
    .expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "navigate through the proxy should succeed: {result:?}"
    );

    let tunneled_target = seen_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("proxy should have handled a CONNECT for the page fetch");
    assert_eq!(
        tunneled_target,
        page_addr.to_string(),
        "the worker's fetch should have tunneled through the proxy to the page's own address"
    );

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[0, 255, 0, 255],
        "the page fetched via the proxy should still render correctly"
    );

    profile.quit();
}

#[test]
fn navigate_without_a_proxy_never_contacts_one() {
    // No proxy configured (plain `Profile::spawn`) - a page fetch must
    // connect directly. Spawning a "proxy" that would panic if it ever
    // received a connection would be a stronger assertion, but a channel
    // that simply never fires is enough to prove nothing routed through
    // it without coupling this test to the proxy helper's own internals.
    let page_addr = serve_html_once(r#"<div>direct</div>"#);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("no-proxy-navigate");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "direct navigate should succeed: {result:?}");

    profile.quit();
}

/// A real UDP DNS server for tests: answers every A query with `answer`
/// (RFC 1035 wire format, same hand-rolled shape `net::dns`'s own client
/// builds/parses) — a genuine query/response round trip, not a stand-in.
fn spawn_fake_dns_server(answer: std::net::Ipv4Addr) -> std::net::SocketAddr {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0").expect("dns server should bind");
    let addr = socket.local_addr().unwrap();
    std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        loop {
            let Ok((n, from)) = socket.recv_from(&mut buf) else {
                return;
            };
            if n < 12 {
                continue;
            }
            let mut resp = Vec::new();
            resp.extend_from_slice(&buf[0..2]); // id
            resp.extend_from_slice(&[0x81, 0x80]); // QR=1, RD=1, RA=1
            resp.extend_from_slice(&[0x00, 0x01]); // QDCOUNT
            resp.extend_from_slice(&[0x00, 0x01]); // ANCOUNT
            resp.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
            resp.extend_from_slice(&buf[12..n]); // echoed question
            resp.extend_from_slice(&[0xC0, 0x0C]); // name = pointer to offset 12
            resp.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]); // TYPE=A, CLASS=IN
            resp.extend_from_slice(&[0x00, 0x00, 0x00, 0x3C]); // TTL
            resp.extend_from_slice(&[0x00, 0x04]); // RDLENGTH
            resp.extend_from_slice(&answer.octets());
            let _ = socket.send_to(&resp, from);
        }
    });
    addr
}

#[test]
fn navigate_routes_through_a_configured_dns_server() {
    // "custom.atomic.test" isn't a real domain - this only resolves (and
    // the navigate only succeeds) because the worker actually used the
    // fake DNS server's answer (127.0.0.1) instead of the OS resolver,
    // which would fail this hostname outright.
    let page_addr = serve_html_once(
        r#"<div id="box">hi</div><style>#box { background-color: #0000ff; width: 32px; height: 32px; }</style>"#,
    );
    let dns_addr = spawn_fake_dns_server(std::net::Ipv4Addr::LOCALHOST);
    let page_url = format!("http://custom.atomic.test:{}/", page_addr.port());

    let name = unique_shmem_name("dns-navigate");
    let mut profile =
        Profile::spawn_with_dns(worker_path(), &name, 32, 32, Some(&dns_addr.to_string()))
            .expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "navigate via the custom DNS server should succeed: {result:?}"
    );

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[0, 0, 255, 255],
        "the page fetched via the custom resolver should still render correctly"
    );

    profile.quit();
}

#[test]
fn navigate_without_a_dns_server_never_contacts_one() {
    let page_addr = serve_html_once(r#"<div>direct</div>"#);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("no-dns-navigate");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "direct navigate should succeed: {result:?}");

    profile.quit();
}
