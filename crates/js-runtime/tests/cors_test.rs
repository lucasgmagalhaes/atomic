use js_runtime::{Context, Runtime};

fn pump_until<F: Fn(&Context) -> bool>(
    ctx: &Context,
    predicate: F,
    timeout: std::time::Duration,
) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        ctx.run_pending_timers();
        if predicate(ctx) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// A real (hand-rolled) HTTP/1.1 server on loopback, serving `body` with
/// `extra_headers` appended verbatim to exactly one request - same
/// convention `profile::tests::serve_html_once` already uses, extended
/// with custom response headers since these tests need to control
/// `Access-Control-Allow-Origin`.
fn serve_once_with_headers(body: &'static str, extra_headers: String) -> std::net::SocketAddr {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n{extra_headers}Connection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

#[test]
fn same_origin_fetch_sync_is_always_allowed() {
    let addr = serve_once_with_headers("hello", String::new());
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url(&url);

    let result = ctx
        .eval(&format!("fetchSync('{url}').ok"), "<test>")
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn cross_origin_fetch_sync_without_cors_header_is_blocked() {
    let addr = serve_once_with_headers("hello", String::new());
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("http://totally-different-origin.example/");

    let result = ctx
        .eval(&format!("JSON.stringify(fetchSync('{url}'))"), "<test>")
        .unwrap();
    assert_eq!(result, r#"{"ok":false,"status":0,"body":""}"#);
}

#[test]
fn cross_origin_fetch_sync_with_wildcard_cors_header_is_allowed() {
    let addr = serve_once_with_headers("hello", "Access-Control-Allow-Origin: *\r\n".to_string());
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("http://totally-different-origin.example/");

    let result = ctx
        .eval(&format!("fetchSync('{url}').ok"), "<test>")
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn cross_origin_fetch_sync_with_matching_origin_header_is_allowed() {
    // The page's own origin (arbitrary, never fetched) must exactly equal
    // what the server sends back for this to prove anything real.
    let page_origin = "http://real-page.example";
    let addr = serve_once_with_headers(
        "hello",
        format!("Access-Control-Allow-Origin: {page_origin}\r\n"),
    );
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url(page_origin);

    let result = ctx
        .eval(&format!("fetchSync('{url}').ok"), "<test>")
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn cross_origin_fetch_sync_with_a_different_origin_header_is_still_blocked() {
    let addr = serve_once_with_headers(
        "hello",
        "Access-Control-Allow-Origin: http://someone-else.example\r\n".to_string(),
    );
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("http://real-page.example");

    let result = ctx
        .eval(&format!("fetchSync('{url}').ok"), "<test>")
        .unwrap();
    assert_eq!(result, "false");
}

#[test]
fn a_context_with_no_real_url_allows_every_request() {
    let addr = serve_once_with_headers("hello", String::new());
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d); // no set_url call

    let result = ctx
        .eval(&format!("fetchSync('{url}').ok"), "<test>")
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn cross_origin_xhr_without_cors_header_reports_onerror_not_onload() {
    let addr = serve_once_with_headers("hello", String::new());
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("http://real-page.example");

    ctx.eval(
        &format!(
            "(() => {{ \
                window.__loaded = false; \
                window.__errored = false; \
                const xhr = new XMLHttpRequest(); \
                xhr.open('GET', '{url}'); \
                xhr.onload = () => {{ window.__loaded = true; }}; \
                xhr.onerror = () => {{ window.__errored = true; }}; \
                xhr.send(); \
            }})()"
        ),
        "<test>",
    )
    .unwrap();

    let settled = pump_until(
        &ctx,
        |ctx| {
            ctx.eval("window.__loaded || window.__errored", "<test>")
                .unwrap()
                == "true"
        },
        std::time::Duration::from_secs(5),
    );
    assert!(
        settled,
        "the XHR should have settled (onload or onerror) within the timeout"
    );

    let result = ctx
        .eval("`${window.__loaded},${window.__errored}`", "<test>")
        .unwrap();
    assert_eq!(result, "false,true");
}
