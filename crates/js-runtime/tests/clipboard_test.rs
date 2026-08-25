use js_runtime::{Context, Runtime};
use std::sync::Mutex;
use std::time::Duration;

// The OS clipboard is one real, process-wide (in fact system-wide)
// resource - tests running concurrently on separate threads (Rust's
// default test harness) that each write then read it back would
// otherwise race against each other's writes. This has nothing to do
// with `js-runtime`'s own concurrency (each test still gets its own
// `Runtime`/`Context`) - it's purely "don't touch the one real
// clipboard from two tests at once".
static CLIPBOARD: Mutex<()> = Mutex::new(());

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
    }
}

#[test]
fn write_text_then_read_text_round_trips_through_the_real_os_clipboard() {
    let _guard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval(
        r#"
        globalThis.seen = null;
        navigator.clipboard.writeText('nimble-clipboard-test-value')
            .then(() => navigator.clipboard.readText())
            .then((t) => { seen = t; });
        "#,
        "<test>",
    )
    .unwrap();

    let done = pump_until(
        &ctx,
        |c| c.eval("seen !== null", "<test>").unwrap() == "true",
        Duration::from_secs(5),
    );
    assert!(
        done,
        "clipboard round trip should complete quickly against a real OS clipboard"
    );
    assert_eq!(
        ctx.eval("seen", "<test>").unwrap(),
        "nimble-clipboard-test-value"
    );
}

#[test]
fn write_text_returns_a_real_promise_object() {
    let _guard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    let result = ctx
        .eval(
            "Object.prototype.toString.call(navigator.clipboard.writeText('x'))",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "[object Promise]");
}

#[test]
fn write_text_rejects_on_a_missing_argument() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    ctx.eval("globalThis.err = null; navigator.clipboard.writeText().catch((e) => { err = e.message; });", "<test>").unwrap();

    let done = pump_until(
        &ctx,
        |c| c.eval("err !== null", "<test>").unwrap() == "true",
        Duration::from_secs(2),
    );
    assert!(done);
    assert!(ctx.eval("err.includes('text')", "<test>").unwrap() == "true");
}

#[test]
fn navigator_clipboard_is_present_on_a_plain_context() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert_eq!(
        ctx.eval("typeof navigator.clipboard.writeText", "<test>")
            .unwrap(),
        "function"
    );
    assert_eq!(
        ctx.eval("typeof navigator.clipboard.readText", "<test>")
            .unwrap(),
        "function"
    );
}
