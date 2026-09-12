#![allow(dead_code)]

use std::io::{Read, Write};

use profile::Profile;

/// Starts a tiny real HTTP/1.1 server on loopback serving `html` to
/// exactly one request, then shuts down - a real server (genuine
/// `TcpListener`, genuine HTTP response bytes over a real socket), not a
/// mock, and doesn't depend on external network content staying stable.
pub(crate) fn serve_html_once(html: &str) -> std::net::SocketAddr {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    let body = html.to_string();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            // Drain the client's request before responding - writing a
            // response without ever reading the request risks the OS
            // resetting the connection out from under the client (seen as
            // a "SendRequest" error on hyper's side) instead of a clean
            // request/response exchange.
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

/// Like [`serve_html_once`], but serves multiple exact paths (a page plus
/// whatever it links to relatively) over as many real connections as
/// arrive - needed to test relative-URL resolution, where the `<link>`
/// href and the page it came from must share one real origin.
pub(crate) fn serve_routes(routes: Vec<(&'static str, String)>) -> std::net::SocketAddr {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            let body = routes
                .iter()
                .find(|(route, _)| *route == path)
                .map(|(_, body)| body.clone())
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

/// Like [`serve_routes`], but serves raw bytes with a caller-chosen
/// `Content-Type` instead of `String`/`text/plain` — needed to serve a
/// real PNG (binary, not UTF-8 text) for an `<img>` test.
pub(crate) fn serve_routes_bytes(
    routes: Vec<(&'static str, &'static str, Vec<u8>)>,
) -> std::net::SocketAddr {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            let (content_type, body) = routes
                .iter()
                .find(|(route, _, _)| *route == path)
                .map(|(_, ct, body)| (*ct, body.clone()))
                .unwrap_or(("text/plain", Vec::new()));
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend_from_slice(&body);
            let _ = stream.write_all(&response);
            let _ = stream.flush();
        }
    });
    addr
}

/// A real encoded PNG (via the `image` crate, same decoder `image_decode`
/// itself wraps) - `width` x `height`, every pixel `color` (straight
/// RGBA8). Used to serve a real `<img src>` a test can assert painted
/// pixels against, without depending on an external image file.
pub(crate) fn encode_png(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba(color));
    let mut bytes = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    )
    .expect("encoding a real PNG should not fail");
    bytes
}

/// Like [`serve_routes`], but each route also carries extra *response*
/// headers (name, value pairs) — needed to deliver a real
/// `Content-Security-Policy` header on the document response, which is
/// exactly what `resolve_document` extracts and `Page::load` enforces.
pub(crate) type RouteWithHeaders = (
    &'static str,
    &'static [(&'static str, &'static str)],
    String,
);

pub(crate) fn serve_routes_with_headers(routes: Vec<RouteWithHeaders>) -> std::net::SocketAddr {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            let (extra_headers, body) = routes
                .iter()
                .find(|(route, _, _)| *route == path)
                .map(|(_, headers, body)| (*headers, body.clone()))
                .unwrap_or((&[], String::new()));
            let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
            for (name, value) in extra_headers {
                response.push_str(&format!("{name}: {value}\r\n"));
            }
            response.push_str("\r\n");
            response.push_str(&body);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

/// Like [`serve_routes`], but also increments a per-path hit counter for
/// every real request received — needed to assert a resource referenced
/// twice by one page load (e.g. the same stylesheet `href`ed by two
/// `<link>` tags) is only actually fetched once, proving the worker's
/// per-page-load `ResourceCache` (see `crate::network::ResourceCache` in
/// `profile-worker`) is really deduping, not just present in source.
pub(crate) fn serve_routes_counted(
    routes: Vec<(&'static str, String)>,
    counts: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, usize>>>,
) -> std::net::SocketAddr {
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/")
                .to_string();
            *counts.lock().unwrap().entry(path.clone()).or_insert(0) += 1;
            let body = routes
                .iter()
                .find(|(route, _)| *route == path)
                .map(|(_, body)| body.clone())
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

pub(crate) fn unique_shmem_name(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("atomic-profile-test-{tag}-{nanos}")
}

pub(crate) fn worker_path() -> &'static str {
    env!("CARGO_BIN_EXE_profile-worker")
}

pub(crate) fn wait_for_a_frame(profile: &Profile) -> Vec<u8> {
    // Keep the startup pacing used by the existing integration tests: the
    // worker owns both GPU setup and its command loop, and a fully settled
    // initial paint is the fixture boundary for the tests below.
    for _ in 0..100 {
        if let Some(frame) = profile.latest_frame() {
            return frame;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("worker should publish a frame within 5s");
}

pub(crate) fn wait_for_generation_after(profile: &Profile, previous: u32) -> u32 {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let generation = profile.frame_generation();
        if generation > previous {
            return generation;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("frame generation should advance within 5s from {previous}");
}

pub(crate) fn wait_for_changed_frame(profile: &Profile, previous: &[u8]) -> Vec<u8> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        if let Some(frame) = profile.latest_frame() {
            if frame != previous {
                return frame;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("rendered frame should change within 5s");
}
