use js_runtime::{Context, Runtime};

// ---------------------------------------------------------------------------
// Headers
// ---------------------------------------------------------------------------

#[test]
fn headers_constructor_empty() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval("new Headers().size", "<test>").unwrap();
  assert_eq!(result, "0");
}

#[test]
fn headers_set_and_get() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "var h = new Headers(); h.set('Content-Type', 'text/html'); h.get('content-type')",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "text/html");
}

#[test]
fn headers_get_returns_undefined_for_missing() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval("new Headers().get('X-Missing')", "<test>")
    .unwrap();
  assert_eq!(result, "undefined");
}

#[test]
fn headers_case_insensitive_get() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "var h = new Headers(); h.set('X-Custom', 'val'); h.get('x-custom')",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "val");
}

#[test]
fn headers_has() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "var h = new Headers(); h.set('Accept', '*/*'); h.has('accept')",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "true");
}

#[test]
fn headers_delete() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "var h = new Headers(); h.set('X-Test', '1'); h.delete('x-test'); h.has('x-test')",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "false");
}

#[test]
fn headers_append_adds_duplicate() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "var h = new Headers(); h.append('Set-Cookie', 'a=1'); h.append('Set-Cookie', 'b=2'); h.get('set-cookie')",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "a=1");
}

#[test]
fn headers_size_counts_unique_names() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "var h = new Headers(); h.append('Set-Cookie', 'a'); h.append('Set-Cookie', 'b'); h.set('Accept', '*'); h.size",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "2");
}

#[test]
fn headers_init_from_object() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "var h = new Headers({'Content-Type': 'application/json', 'Accept': 'text'}); h.get('content-type')",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "application/json");
}

#[test]
fn headers_to_string() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "var h = new Headers(); h.set('Accept', '*/*'); h.set('X-Foo', 'bar'); h.toString()",
      "<test>",
    )
    .unwrap();
  // Order may vary, but both headers must be present
  assert!(result.contains("Accept: */*"), "{result}");
  assert!(result.contains("X-Foo: bar"), "{result}");
}

#[test]
fn headers_to_json() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "JSON.stringify(new Headers({'X-A': '1', 'x-a': '2'}))",
      "<test>",
    )
    .unwrap();
  // JSON.stringify on Headers uses toJSON which lowercases and comma-joins
  assert!(result.contains("x-a"), "{result}");
  assert!(result.contains("1, 2"), "{result}");
}

#[test]
fn headers_for_each() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      r#"
      var collected = [];
      var h = new Headers({'A': '1', 'B': '2'});
      h.forEach(function(value, key) { collected.push(key + '=' + value); });
      collected.sort().join(',')
      "#,
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "A=1,B=2");
}

#[test]
fn headers_entries() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      r#"
      var h = new Headers({'X': '1'});
      var e = h.entries();
      e[0][0] + '=' + e[0][1]
      "#,
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "X=1");
}

#[test]
fn headers_keys_and_values() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      r#"
      var h = new Headers({'Accept': 'text/html'});
      var k = h.keys();
      var v = h.values();
      k[0] + ':' + v[0]
      "#,
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "Accept:text/html");
}

#[test]
fn headers_init_from_another_headers() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "var a = new Headers({'X': '1'}); var b = new Headers(a); b.get('x')",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "1");
}

#[test]
fn headers_set_replaces_all_with_same_name() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval(
      "var h = new Headers(); h.append('Set-Cookie', 'a'); h.append('Set-Cookie', 'b'); h.set('set-cookie', 'c'); h.get('set-cookie')",
      "<test>",
    )
    .unwrap();
  assert_eq!(result, "c");
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

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
  ctx
    .eval(
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
  ctx
    .eval(
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
  ctx
    .eval(
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
  ctx
    .eval(
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

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// fetchSync returns Response instances
// ---------------------------------------------------------------------------

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
