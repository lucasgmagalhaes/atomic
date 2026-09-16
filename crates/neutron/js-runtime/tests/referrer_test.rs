use js_runtime::{Context, Runtime};

/// A real (hand-rolled) HTTP/1.1 server on loopback that echoes the raw
/// request line + headers it received back as the response body, so a
/// test can assert on exactly what `Referer` (if any) a request actually
/// carried — same convention `cors_test.rs`'s `serve_once_with_headers`
/// already uses, just echoing the request instead of serving fixed content.
fn serve_once_echoing_request() -> std::net::SocketAddr {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let received = String::from_utf8_lossy(&buf[..n]).into_owned();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                received.len(),
                received
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

#[test]
fn same_origin_request_sends_the_full_page_url_as_referer() {
    let addr = serve_once_echoing_request();
    let url = format!("http://{addr}/target");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    let page_url = format!("http://{addr}/page?x=1");
    ctx.set_url(&page_url);

    ctx.eval(
        &format!("var _b = ''; fetchSync('{url}').text().then(function(t) {{ _b = t; }});"),
        "<test>",
    )
    .unwrap();
    ctx.run_pending_timers();
    let result = ctx.eval("_b", "<test>").unwrap();
    assert!(
        result
            .to_lowercase()
            .contains(&format!("referer: {page_url}").to_lowercase()),
        "got: {result}"
    );
}

#[test]
fn cross_origin_request_sends_only_the_page_origin_as_referer() {
    let addr = serve_once_echoing_request();
    let url = format!("http://{addr}/target");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("http://totally-different-origin.example/secret-path");

    ctx.eval(
        &format!("var _b = ''; fetchSync('{url}').text().then(function(t) {{ _b = t; }});"),
        "<test>",
    )
    .unwrap();
    ctx.run_pending_timers();
    let result = ctx.eval("_b", "<test>").unwrap();
    assert!(
        result
            .to_lowercase()
            .contains("referer: http://totally-different-origin.example/\r\n"),
        "got: {result}"
    );
}

#[test]
fn an_opaque_origin_pages_location_origin_reads_null() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("data:text/html,hello");

    let result = ctx.eval("location.origin", "<test>").unwrap();
    assert_eq!(result, "null");
}

#[test]
fn an_opaque_origin_page_sends_no_referer() {
    let addr = serve_once_echoing_request();
    let url = format!("http://{addr}/target");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("data:text/html,hello");

    ctx.eval(
        &format!("var _b = ''; fetchSync('{url}').text().then(function(t) {{ _b = t; }});"),
        "<test>",
    )
    .unwrap();
    ctx.run_pending_timers();
    let result = ctx.eval("_b", "<test>").unwrap();
    assert!(!result.to_lowercase().contains("referer:"), "got: {result}");
}

#[test]
fn a_context_with_no_real_url_sends_no_referer() {
    let addr = serve_once_echoing_request();
    let url = format!("http://{addr}/target");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d); // no set_url call

    ctx.eval(
        &format!("var _b = ''; fetchSync('{url}').text().then(function(t) {{ _b = t; }});"),
        "<test>",
    )
    .unwrap();
    ctx.run_pending_timers();
    let result = ctx.eval("_b", "<test>").unwrap();
    assert!(!result.to_lowercase().contains("referer:"), "got: {result}");
}
