use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::thread;

/// A real (hand-rolled) UDP DNS server that answers every A query with
/// `answer` - runs on a background thread until the socket is dropped
/// (recv fails).
fn spawn_fake_dns_server(answer: Ipv4Addr) -> SocketAddr {
  let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
  let addr = socket.local_addr().unwrap();

  thread::spawn(move || {
    let mut buf = [0u8; 512];
    loop {
      let Ok((n, from)) = socket.recv_from(&mut buf) else {
        return;
      };
      if n < 12 {
        continue;
      }
      let id = [buf[0], buf[1]];

      // Echo back a minimal well-formed response: header (QR=1,
      // RD+RA, 1 question, 1 answer), the original question section
      // verbatim (a compression pointer back to offset 0 would also
      // work, but copying it is simpler and just as valid), then
      // one A-record answer pointing at offset 12 (the question's
      // QNAME) with `answer`'s 4 bytes as RDATA.
      let mut resp = Vec::new();
      resp.extend_from_slice(&id);
      resp.extend_from_slice(&[0x81, 0x80]); // QR=1, RD=1, RA=1
      resp.extend_from_slice(&[0x00, 0x01]); // QDCOUNT = 1
      resp.extend_from_slice(&[0x00, 0x01]); // ANCOUNT = 1
      resp.extend_from_slice(&[0x00, 0x00]);
      resp.extend_from_slice(&[0x00, 0x00]);
      resp.extend_from_slice(&buf[12..n]); // original question section

      resp.extend_from_slice(&[0xC0, 0x0C]); // name = pointer to offset 12
      resp.extend_from_slice(&[0x00, 0x01]); // TYPE = A
      resp.extend_from_slice(&[0x00, 0x01]); // CLASS = IN
      resp.extend_from_slice(&[0x00, 0x00, 0x00, 0x3C]); // TTL = 60
      resp.extend_from_slice(&[0x00, 0x04]); // RDLENGTH = 4
      resp.extend_from_slice(&answer.octets());

      let _ = socket.send_to(&resp, from);
    }
  });

  addr
}

fn spawn_fake_http_server(body: &'static str) -> u16 {
  let listener = TcpListener::bind("127.0.0.1:0").unwrap();
  let port = listener.local_addr().unwrap().port();

  thread::spawn(move || {
    if let Ok((mut stream, _)) = listener.accept() {
      let mut buf = [0u8; 1024];
      let _ = stream.read(&mut buf);
      let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
      );
      let _ = stream.write_all(response.as_bytes());
    }
  });

  port
}

#[tokio::test]
async fn resolve_a_returns_the_real_answer_from_a_local_dns_server() {
  let dns_addr = spawn_fake_dns_server(Ipv4Addr::new(203, 0, 113, 42));
  let ip = net::resolve_a("custom.nimble.test", dns_addr)
    .await
    .expect("resolution should succeed");
  assert_eq!(ip, Ipv4Addr::new(203, 0, 113, 42));
}

#[tokio::test]
async fn resolve_a_times_out_against_a_server_that_never_answers() {
  // A bound-but-silent socket: nothing ever replies.
  let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
  let addr = silent.local_addr().unwrap();
  // Keep the socket alive on this thread for the query's duration so
  // the OS doesn't ICMP-port-unreachable it into an instant error -
  // we want an actual timeout, not a fast failure.
  let _keep_alive = silent;

  let result = tokio::time::timeout(
    std::time::Duration::from_secs(7),
    net::resolve_a("nimble.test", addr),
  )
  .await;
  assert!(
    result.is_ok(),
    "resolve_a itself should time out internally, not hang forever"
  );
  assert!(result.unwrap().is_err());
}

#[test]
fn get_via_dns_uses_the_custom_resolver_not_the_os_one() {
  // "custom.nimble.test" isn't a real domain - if this succeeds, the
  // request only could have worked because get_via_dns actually used
  // our fake DNS server's answer (127.0.0.1) instead of asking the OS
  // resolver (which would fail to resolve it at all).
  let dns_addr = spawn_fake_dns_server(Ipv4Addr::LOCALHOST);
  let http_port = spawn_fake_http_server("hello from custom dns");
  std::thread::sleep(std::time::Duration::from_millis(50));

  let url = format!("http://custom.nimble.test:{http_port}/");
  let response =
    net::get_via_dns(&url, &[], dns_addr).expect("request should succeed via the custom resolver");

  assert_eq!(response.status, 200);
  assert_eq!(
    String::from_utf8_lossy(&response.body),
    "hello from custom dns"
  );
}

#[test]
fn get_via_dns_fails_when_the_dns_server_has_no_answer() {
  // A silent UDP socket never answers, so resolution should time out
  // and the whole request should fail - not silently fall back to the
  // OS resolver.
  let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
  let addr = silent.local_addr().unwrap();
  let _keep_alive = silent;

  let result = net::get_via_dns("http://custom.nimble.test/", &[], addr);
  assert!(result.is_err());
}
