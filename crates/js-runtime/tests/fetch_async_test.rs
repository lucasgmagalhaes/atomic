use std::thread::sleep;
use std::time::Duration;

use js_runtime::{Context, Runtime};

fn pump_until<F: Fn(&Context) -> bool>(ctx: &Context, predicate: F, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        ctx.run_pending_timers();
        if predicate(ctx) {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        sleep(Duration::from_millis(10));
    }
}

#[test]
fn fetch_returns_a_real_promise_object() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx.eval("Object.prototype.toString.call(fetch('https://example.com/'))", "<test>").unwrap();
    assert_eq!(result, "[object Promise]");
}

#[test]
fn fetch_does_not_resolve_before_the_background_request_completes() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        "globalThis.resolved = false; fetch('https://example.com/').then(() => { resolved = true; });",
        "<test>",
    )
    .unwrap();
    assert_eq!(ctx.eval("resolved", "<test>").unwrap(), "false");
}

#[test]
fn fetch_then_resolves_with_a_real_response_once_pumped() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.seen = null;
        fetch('https://example.com/').then((res) => { seen = res; });
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("seen !== null", "<test>").unwrap() == "true", Duration::from_secs(15));
    assert!(done, "fetch should resolve within 15s against a real network");

    assert_eq!(ctx.eval("seen.ok", "<test>").unwrap(), "true");
    assert_eq!(ctx.eval("seen.status", "<test>").unwrap(), "200");
    assert_eq!(ctx.eval("seen.body.includes('Example Domain')", "<test>").unwrap(), "true");
}

#[test]
fn fetch_rejects_on_a_missing_url_argument() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval("globalThis.err = null; fetch().catch((e) => { err = e; });", "<test>").unwrap();

    let done = pump_until(&ctx, |c| c.eval("err !== null", "<test>").unwrap() == "true", Duration::from_secs(2));
    assert!(done);
    assert!(ctx.eval("err.includes('url')", "<test>").unwrap() == "true");
}

#[test]
fn async_await_over_fetch_works_once_pumped() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.status = null;
        (async () => {
            const res = await fetch('https://example.com/');
            status = res.status;
        })();
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("status !== null", "<test>").unwrap() == "true", Duration::from_secs(15));
    assert!(done, "async/await over a real fetch should complete within 15s");
    assert_eq!(ctx.eval("status", "<test>").unwrap(), "200");
}

#[test]
fn xhr_open_sets_ready_state_to_opened() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            r#"
            const xhr = new XMLHttpRequest();
            xhr.open('GET', 'https://example.com/');
            xhr.readyState;
            "#,
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1");
}

#[test]
fn xhr_send_completes_via_onload_once_pumped() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.loaded = false;
        globalThis.xhr = new XMLHttpRequest();
        xhr.open('GET', 'https://example.com/');
        xhr.onload = function () { loaded = true; };
        xhr.send();
        "#,
        "<test>",
    )
    .unwrap();

    assert_eq!(ctx.eval("loaded", "<test>").unwrap(), "false");

    let done = pump_until(&ctx, |c| c.eval("loaded", "<test>").unwrap() == "true", Duration::from_secs(15));
    assert!(done, "xhr should complete within 15s against a real network");

    assert_eq!(ctx.eval("xhr.readyState", "<test>").unwrap(), "4");
    assert_eq!(ctx.eval("xhr.status", "<test>").unwrap(), "200");
    assert_eq!(ctx.eval("xhr.responseText.includes('Example Domain')", "<test>").unwrap(), "true");
    // `onload` is called with `this` bound to the xhr instance.
    let this_check = ctx
        .eval(
            r#"
            globalThis.sawThis = null;
            const xhr2 = new XMLHttpRequest();
            xhr2.open('GET', 'https://example.com/');
            xhr2.onload = function () { sawThis = (this === xhr2); };
            xhr2.send();
            "#,
            "<test>",
        )
        .is_ok();
    assert!(this_check);
}

#[test]
fn xhr_this_binding_inside_onload_is_the_instance() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.sawThis = null;
        globalThis.xhr = new XMLHttpRequest();
        xhr.open('GET', 'https://example.com/');
        xhr.onload = function () { sawThis = (this === xhr); };
        xhr.send();
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(&ctx, |c| c.eval("sawThis !== null", "<test>").unwrap() == "true", Duration::from_secs(15));
    assert!(done);
    assert_eq!(ctx.eval("sawThis", "<test>").unwrap(), "true");
}
