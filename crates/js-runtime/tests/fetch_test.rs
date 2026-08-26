use js_runtime::{Context, Runtime};

#[test]
fn fetch_sync_missing_url_returns_not_ok() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval("fetchSync().ok", "<test>").unwrap();
  assert_eq!(result, "false");
}

#[test]
fn fetch_sync_missing_url_returns_zero_status() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval("fetchSync().status", "<test>").unwrap();
  assert_eq!(result, "0");
}

#[test]
fn fetch_sync_bad_url_returns_not_ok() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval("fetchSync('not-a-url').ok", "<test>").unwrap();
  assert_eq!(result, "false");
}

#[test]
fn fetch_sync_returns_response_instance() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval("fetchSync('not-a-url') instanceof Response", "<test>")
    .unwrap();
  assert_eq!(result, "true");
}

#[test]
fn fetch_sync_bad_url_returns_empty_body() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  ctx
    .eval(
      "var _r = ''; fetchSync('not-a-url').text().then(function(v) { _r = v; });",
      "<test>",
    )
    .unwrap();
  // text() returns a Promise; settled_promise resolves it immediately but
  // the .then() callback still needs a job-queue drain to actually run.
  ctx.run_pending_timers();
  let result = ctx.eval("_r", "<test>").unwrap();
  assert_eq!(result, "");
}

#[test]
fn response_has_headers_property() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval("typeof fetchSync('bad').headers", "<test>")
    .unwrap();
  assert_eq!(result, "object");
}

#[test]
fn response_status_and_ok_reflect_status_code() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  // Missing URL -> status 0, ok false
  let result = ctx
    .eval("var r = fetchSync(); [r.ok, r.status]", "<test>")
    .unwrap();
  assert_eq!(result, "false,0");
}

// Ignored by default: hits the real network. Run explicitly with
// `cargo test -- --ignored` on a machine with internet access.
#[test]
#[ignore]
fn fetch_sync_round_trips_a_real_https_request() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let ok = ctx
    .eval("fetchSync('https://example.com').ok", "<test>")
    .unwrap();
  assert_eq!(ok, "true");
}
