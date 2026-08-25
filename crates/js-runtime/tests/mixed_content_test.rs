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

/// A real (hand-rolled) plain-HTTP server on loopback, serving `body` to
/// exactly one request — if a mixed-content check is ever accidentally
/// skipped, a test using this would still see the request actually land
/// here rather than failing for an unrelated reason.
fn serve_once(body: &'static str) -> std::net::SocketAddr {
    use std::io::{Read, Write};
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    addr
}

#[test]
fn fetch_sync_blocks_a_plain_http_request_from_an_https_page() {
    let addr = serve_once("hello");
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://secure-page.example/");

    let result = ctx
        .eval(&format!("JSON.stringify(fetchSync('{url}'))"), "<test>")
        .unwrap();
    assert_eq!(result, r#"{"ok":false,"status":0,"body":""}"#);
}

#[test]
fn fetch_sync_allows_a_plain_http_request_from_an_http_page() {
    let addr = serve_once("hello");
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url(&url);

    let result = ctx
        .eval(&format!("fetchSync('{url}').ok"), "<test>")
        .unwrap();
    assert_eq!(
        result, "true",
        "an http:// page fetching its own http:// origin is never mixed content"
    );
}

#[test]
fn fetch_promise_rejects_for_mixed_content() {
    let addr = serve_once("hello");
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://secure-page.example/");

    ctx.eval(
        &format!(
            "(() => {{ \
                window.__resolved = false; \
                window.__rejected = false; \
                fetch('{url}').then(() => {{ window.__resolved = true; }}, () => {{ window.__rejected = true; }}); \
            }})()"
        ),
        "<test>",
    )
    .unwrap();

    let settled = pump_until(
        &ctx,
        |ctx| {
            ctx.eval("window.__resolved || window.__rejected", "<test>")
                .unwrap()
                == "true"
        },
        std::time::Duration::from_secs(5),
    );
    assert!(settled, "the fetch should have settled within the timeout");

    let result = ctx
        .eval("`${window.__resolved},${window.__rejected}`", "<test>")
        .unwrap();
    assert_eq!(result, "false,true");
}

#[test]
fn xhr_reports_onerror_for_mixed_content() {
    let addr = serve_once("hello");
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("https://secure-page.example/");

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
    assert!(settled, "the XHR should have settled within the timeout");

    let result = ctx
        .eval("`${window.__loaded},${window.__errored}`", "<test>")
        .unwrap();
    assert_eq!(result, "false,true");
}
