use js_runtime::{Context, Runtime};

#[test]
fn fetch_sync_missing_url_returns_not_ok() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx.eval("fetchSync().ok", "<test>").unwrap();
  assert_eq!(result, "false");
}

#[test]
fn fetch_sync_bad_url_returns_not_ok_with_zero_status() {
  let rt = Runtime::new();
  let ctx = Context::new(&rt);
  let result = ctx
    .eval("JSON.stringify(fetchSync('not-a-url'))", "<test>")
    .unwrap();
  assert_eq!(result, r#"{"ok":false,"status":0,"body":""}"#);
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
