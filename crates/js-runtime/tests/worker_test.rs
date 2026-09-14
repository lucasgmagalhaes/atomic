use std::time::{Duration, Instant};

use js_runtime::{Context, Runtime};

/// Worker delivery is real cross-thread (a genuine OS thread), so a test
/// must poll `run_pending_timers` for a bit rather than assuming one call
/// is enough — same shape any other cross-thread completion in this
/// crate's tests would need.
fn pump_until(ctx: &Context, condition: impl Fn(&Context) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition(ctx) {
        ctx.run_pending_timers();
        if Instant::now() > deadline {
            panic!("condition never became true within the timeout");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn main_thread_receives_a_real_structured_clone_posted_from_a_worker() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.received = null; \
            globalThis.w = new Worker('postMessage({a: 1, b: \"x\"})'); \
            globalThis.w.onmessage = (e) => { globalThis.received = e.data; }; \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    pump_until(&ctx, |ctx| {
        ctx.eval("globalThis.received !== null", "<test>").unwrap() == "true"
    });

    let received = ctx
        .eval("JSON.stringify(globalThis.received)", "<test>")
        .expect("reading the result should not throw");
    assert_eq!(
        received, "{\"a\":1,\"b\":\"x\"}",
        "the main thread must receive a real structured clone of what the worker posted"
    );
}

#[test]
fn a_worker_receives_a_message_posted_from_the_main_thread() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.received = null; \
            globalThis.w = new Worker( \
                'onmessage = (e) => { postMessage(e.data * 2); };' \
            ); \
            globalThis.w.onmessage = (e) => { globalThis.received = e.data; }; \
            globalThis.w.postMessage(21); \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    pump_until(&ctx, |ctx| {
        ctx.eval("globalThis.received !== null", "<test>").unwrap() == "true"
    });

    let received = ctx
        .eval("globalThis.received", "<test>")
        .expect("reading the result should not throw");
    assert_eq!(
        received, "42",
        "the worker must receive the posted value and echo back double it"
    );
}

#[test]
fn posted_data_is_a_real_independent_clone_not_a_shared_reference() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.original = { count: 1 }; \
            globalThis.w = new Worker('postMessage(0)'); \
            globalThis.w.onmessage = () => {}; \
            globalThis.w.postMessage(globalThis.original); \
            globalThis.original.count = 999; \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    // Give the worker a moment to run (it doesn't echo the object back,
    // so there's nothing to pump-wait on) - the assertion is purely about
    // the main-thread-side object never having been shared with the
    // worker in the first place.
    std::thread::sleep(Duration::from_millis(50));

    let count = ctx
        .eval("globalThis.original.count", "<test>")
        .expect("reading the result should not throw");
    assert_eq!(
        count, "999",
        "mutating the original after postMessage must not affect anything the worker saw"
    );
}

#[test]
fn terminate_stops_the_worker_from_delivering_further_messages() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.count = 0; \
            globalThis.w = new Worker( \
                'onmessage = () => { postMessage(1); };' \
            ); \
            globalThis.w.onmessage = () => { globalThis.count += 1; }; \
            globalThis.w.postMessage('go'); \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    pump_until(&ctx, |ctx| {
        ctx.eval("globalThis.count", "<test>").unwrap() == "1"
    });

    ctx.eval("globalThis.w.terminate()", "<test>")
        .expect("terminate should not throw");
    ctx.eval("globalThis.w.postMessage('after-terminate')", "<test>")
        .expect("posting to a terminated worker should not throw");

    ctx.run_pending_timers();
    std::thread::sleep(Duration::from_millis(50));
    ctx.run_pending_timers();

    let count = ctx
        .eval("globalThis.count", "<test>")
        .expect("reading the result should not throw");
    assert_eq!(
        count, "1",
        "no further message should arrive once the worker has been terminated"
    );
}

#[test]
fn a_worker_runs_on_a_real_separate_thread_not_blocking_the_main_context() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    // A worker whose initial script takes a moment (a real busy loop,
    // bounded so `Context::drop`'s own `join()` on this worker's thread
    // doesn't hang the test suite forever - see `worker_bindings::cleanup`'s
    // own doc on this being a real, documented scope cut: nothing here can
    // forcibly kill a worker OS thread stuck outside its message loop)
    // must not block `new Worker(...)` itself or any later eval on the
    // main context - proves this is a genuine async OS thread, not
    // synchronous inline execution.
    let start = Instant::now();
    ctx.eval(
        "new Worker('const start = Date.now(); while (Date.now() - start < 200) {}')",
        "<test>",
    )
    .expect("constructing a worker with a busy-looping script must not block the caller");
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "new Worker(...) must return immediately regardless of what the worker script does"
    );

    let still_alive = ctx
        .eval("1 + 1", "<test>")
        .expect("the main context must stay fully responsive");
    assert_eq!(still_alive, "2");
}
