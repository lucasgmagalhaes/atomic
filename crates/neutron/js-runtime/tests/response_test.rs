use js_runtime::{Context, Runtime};

#[test]
fn response_constructor_defaults() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "var r = new Response(); [r.ok, r.status, r.statusText]",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,200,OK");
}

#[test]
fn response_with_body() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
      var _t = '';
      new Response('hello').text().then(function(v) { _t = v; });
      "#,
        "<test>",
    )
    .unwrap();
    // settled_promise resolves immediately; need to drain jobs
    ctx.run_pending_timers();
    let result = ctx.eval("_t", "<test>").unwrap();
    assert_eq!(result, "hello");
}

#[test]
fn response_custom_status() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
    .eval(
      "var r = new Response(null, {status: 404, statusText: 'Not Found'}); [r.ok, r.status, r.statusText]",
      "<test>",
    )
    .unwrap();
    assert_eq!(result, "false,404,Not Found");
}

#[test]
fn response_with_headers() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "var r = new Response(null, {headers: {'X-Custom': 'yes'}}); r.headers.get('x-custom')",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "yes");
}

#[test]
fn response_type_is_basic() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("new Response().type", "<test>").unwrap();
    assert_eq!(result, "basic");
}

#[test]
fn response_body_used_is_false() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("new Response().bodyUsed", "<test>").unwrap();
    assert_eq!(result, "false");
}

#[test]
fn response_clone() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            r#"
      var r = new Response('data', {status: 201});
      var c = r.clone();
      [c.ok, c.status] + ''
      "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,201");
}

#[test]
fn response_json() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
    var _v = '';
    new Response('{"a":1}').json().then(function(o) { _v = o.a; });
    "#,
        "<test>",
    )
    .unwrap();
    ctx.run_pending_timers();
    let result = ctx.eval("_v", "<test>").unwrap();
    assert_eq!(result, "1");
}

#[test]
fn response_json_rejects_invalid() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
    var _err = '';
    new Response('not json').json().catch(function(e) { _err = e.message; });
    "#,
        "<test>",
    )
    .unwrap();
    ctx.run_pending_timers();
    let result = ctx.eval("_err", "<test>").unwrap();
    assert_eq!(result, "invalid JSON");
}

#[test]
fn response_array_buffer() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
    var _len = -1;
    new Response('hello').arrayBuffer().then(function(b) { _len = b.byteLength; });
    "#,
        "<test>",
    )
    .unwrap();
    ctx.run_pending_timers();
    let result = ctx.eval("_len", "<test>").unwrap();
    assert_eq!(result, "5");
}
