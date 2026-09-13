use js_runtime::{Context, Runtime};

#[test]
fn request_constructor_basic() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "var r = new Request('https://example.com'); [r.url, r.method]",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "https://example.com,GET");
}

#[test]
fn request_default_method_is_get() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("new Request('http://x').method", "<test>")
        .unwrap();
    assert_eq!(result, "GET");
}

#[test]
fn request_custom_method() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("new Request('http://x', {method: 'POST'}).method", "<test>")
        .unwrap();
    assert_eq!(result, "POST");
}

#[test]
fn request_body_used_is_false() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("new Request('http://x').bodyUsed", "<test>")
        .unwrap();
    assert_eq!(result, "false");
}

#[test]
fn request_has_headers() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval("typeof new Request('http://x').headers", "<test>")
        .unwrap();
    assert_eq!(result, "object");
}

#[test]
fn request_clone() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            r#"
      var r = new Request('http://x', {method: 'DELETE'});
      var c = r.clone();
      c.method
      "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "DELETE");
}

#[test]
fn request_from_request_object() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            r#"
      var a = new Request('http://x', {method: 'PUT'});
      var b = new Request(a);
      [b.url, b.method] + ''
      "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "http://x,PUT");
}
