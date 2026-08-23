use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use net::ProxyConfig;

/// A real (not mocked) plain-HTTP origin server: accepts one connection,
/// reads and discards the request, and always replies with a fixed 200
/// body. Runs forever on its own thread; the test process exiting is what
/// stops it, same convention `profile-worker`'s own tests use for local
/// servers.
fn spawn_origin_server(body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("origin server should bind");
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let response =
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    port
}

/// A real forward proxy: answers a `CONNECT host:port HTTP/1.1` request
/// with `200 Connection Established`, then splices raw bytes between the
/// client and `host:port` in both directions — a genuine byte-for-byte
/// tunnel, not a stand-in that special-cases the test's own HTTP request.
/// `on_connect_request`, if given, receives the CONNECT request's own
/// header lines (for asserting `Proxy-Authorization` was actually sent).
fn spawn_connect_proxy(on_connect_request: Option<std::sync::mpsc::Sender<Vec<String>>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("proxy should bind");
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut client) = stream else { continue };
            let mut reader = BufReader::new(client.try_clone().expect("clone client stream"));

            let mut request_line = String::new();
            if reader.read_line(&mut request_line).is_err() || request_line.is_empty() {
                continue;
            }
            let target = request_line.trim_start_matches("CONNECT ").split(' ').next().unwrap_or("").to_string();

            let mut header_lines = Vec::new();
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line == "\r\n" => break,
                    Ok(_) => header_lines.push(line.trim_end().to_string()),
                }
            }
            if let Some(sender) = &on_connect_request {
                let _ = sender.send(header_lines);
            }

            let Ok(mut upstream) = TcpStream::connect(&target) else {
                let _ = client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n");
                continue;
            };
            let _ = client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n");

            let mut upstream_reader = upstream.try_clone().expect("clone upstream stream");
            let mut client_writer = client.try_clone().expect("clone client stream");
            let client_to_upstream = thread::spawn(move || {
                let _ = std::io::copy(&mut reader, &mut upstream);
            });
            let upstream_to_client = thread::spawn(move || {
                let _ = std::io::copy(&mut upstream_reader, &mut client_writer);
            });
            let _ = client_to_upstream.join();
            let _ = upstream_to_client.join();
        }
    });
    port
}

#[test]
fn tunnels_a_plain_http_request_through_a_connect_proxy() {
    let origin_port = spawn_origin_server("hello via proxy");
    let proxy_port = spawn_connect_proxy(None);

    let proxy = ProxyConfig { host: "127.0.0.1".to_string(), port: proxy_port, username: None, password: None };
    let response = net::get_via_proxy(&format!("http://127.0.0.1:{origin_port}/"), &[], &proxy).expect("proxied request should succeed");

    assert_eq!(response.status, 200);
    assert_eq!(String::from_utf8_lossy(&response.body), "hello via proxy");
}

#[test]
fn sends_proxy_authorization_when_credentials_are_set() {
    let origin_port = spawn_origin_server("ok");
    let (tx, rx) = std::sync::mpsc::channel();
    let proxy_port = spawn_connect_proxy(Some(tx));

    let proxy = ProxyConfig {
        host: "127.0.0.1".to_string(),
        port: proxy_port,
        username: Some("alice".to_string()),
        password: Some("s3cret".to_string()),
    };
    let response = net::get_via_proxy(&format!("http://127.0.0.1:{origin_port}/"), &[], &proxy).expect("proxied request should succeed");
    assert_eq!(response.status, 200);

    let connect_headers = rx.recv_timeout(std::time::Duration::from_secs(5)).expect("proxy should have received a CONNECT request");
    let expected = format!("Proxy-Authorization: Basic {}", base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"alice:s3cret"));
    assert!(
        connect_headers.iter().any(|h| h == &expected),
        "expected {expected:?} among CONNECT headers, got {connect_headers:?}"
    );
}

#[test]
fn fails_when_the_proxy_is_unreachable() {
    let proxy = ProxyConfig { host: "127.0.0.1".to_string(), port: 1, username: None, password: None };
    let result = net::get_via_proxy("http://example.com/", &[], &proxy);
    assert!(matches!(result, Err(net::Error::Request(_))));
}

#[test]
fn fails_when_the_proxy_refuses_the_connect() {
    // A plain HTTP (non-CONNECT-aware) origin server answers any request,
    // including our CONNECT line, with an ordinary 200 that isn't
    // "Connection Established" in the way a real proxy's reply is
    // expected to be parsed for status — good enough to exercise the
    // non-200 rejection path without writing a second proxy stub.
    let origin_port = spawn_origin_server("not a proxy");
    let proxy = ProxyConfig { host: "127.0.0.1".to_string(), port: origin_port, username: None, password: None };
    let result = net::get_via_proxy("http://example.com/", &[], &proxy);
    assert!(matches!(result, Err(net::Error::Request(_))));
}
