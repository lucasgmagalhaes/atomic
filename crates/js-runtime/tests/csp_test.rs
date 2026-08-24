use js_runtime::{Context, Runtime};

fn serve_once(body: &'static str) -> std::net::SocketAddr {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("failed to bind a loopback test server");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
fn a_policy_with_no_connect_src_or_default_src_allows_everything() {
    let addr = serve_once("hello");
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("http://page.example/");
    ctx.set_csp("script-src 'self'");

    let result = ctx.eval(&format!("fetchSync('{url}').ok"), "<test>").unwrap();
    assert_eq!(result, "true");
}

#[test]
fn connect_src_none_blocks_every_fetch() {
    let addr = serve_once("hello");
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("http://page.example/");
    ctx.set_csp("connect-src 'none'");

    let result = ctx.eval(&format!("JSON.stringify(fetchSync('{url}'))"), "<test>").unwrap();
    assert_eq!(result, r#"{"ok":false,"status":0,"body":""}"#);
}

#[test]
fn connect_src_self_allows_same_origin_and_blocks_cross_origin() {
    let addr = serve_once("hello");
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url(&url);
    ctx.set_csp("connect-src 'self'");

    let same_origin = ctx.eval(&format!("fetchSync('{url}').ok"), "<test>").unwrap();
    assert_eq!(same_origin, "true");

    ctx.set_url("http://totally-different-origin.example/");
    let cross_origin = ctx.eval(&format!("fetchSync('{url}').ok"), "<test>").unwrap();
    assert_eq!(cross_origin, "false");
}

#[test]
fn connect_src_lists_an_explicit_allowed_host() {
    let addr = serve_once("hello");
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("http://page.example/");
    ctx.set_csp(&format!("connect-src {}", addr.ip()));

    let result = ctx.eval(&format!("fetchSync('{url}').ok"), "<test>").unwrap();
    assert_eq!(result, "true");
}

#[test]
fn a_context_with_no_csp_set_allows_everything() {
    let addr = serve_once("hello");
    let url = format!("http://{addr}/");

    let d = dom::Dom::new();
    let rt = Runtime::new();
    let mut ctx = Context::with_dom(&rt, d);
    ctx.set_url("http://page.example/"); // no set_csp call

    let result = ctx.eval(&format!("fetchSync('{url}').ok"), "<test>").unwrap();
    assert_eq!(result, "true");
}
