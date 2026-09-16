use js_runtime::{Context, Runtime};

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
