use js_runtime::{Context, Runtime};

#[test]
fn fetch_sync_returns_response_instance() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("fetchSync('bad-url') instanceof Response", "<test>")
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn fetch_sync_response_has_ok_status() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("fetchSync().ok", "<test>").unwrap();
    assert_eq!(result, "false");
}

#[test]
fn fetch_sync_response_headers_is_headers_instance() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("fetchSync('bad').headers instanceof Headers", "<test>")
        .unwrap();
    assert_eq!(result, "true");
}
