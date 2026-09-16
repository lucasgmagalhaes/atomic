use std::time::{Duration, Instant};

use js_runtime::{Context, Runtime};

/// Same cross-thread poll shape `worker_test.rs`'s own `pump_until`
/// already uses — `SharedWorker` delivery is real cross-thread too.
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
fn a_connecting_context_gets_a_real_port_and_the_worker_gets_onconnect() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.received = null; \
            globalThis.sw = new SharedWorker( \
                'self.onconnect = (e) => { \
                    const port = e.ports[0]; \
                    port.onmessage = (ev) => { port.postMessage(ev.data * 2); }; \
                };' \
            ); \
            globalThis.sw.port.onmessage = (e) => { globalThis.received = e.data; }; \
            globalThis.sw.port.postMessage(21); \
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
        "a real onconnect must fire with a working port that can reply"
    );
}

#[test]
fn two_shared_worker_calls_with_the_same_script_reuse_one_instance() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    // The worker script keeps a module-level counter that increments once
    // per `onconnect` - if two `new SharedWorker(sameScript)` calls really
    // share one instance/thread, that counter is shared state and the
    // second connection observes it already at 1.
    let script = "\
        globalThis.__connections = 0; \
        self.onconnect = (e) => { \
            globalThis.__connections += 1; \
            const port = e.ports[0]; \
            port.postMessage(globalThis.__connections); \
        };";

    ctx.eval(
        &format!(
            "(() => {{ \
                globalThis.a = null; \
                globalThis.b = null; \
                globalThis.sw1 = new SharedWorker('{script}'); \
                globalThis.sw1.port.onmessage = (e) => {{ globalThis.a = e.data; }}; \
                globalThis.sw2 = new SharedWorker('{script}'); \
                globalThis.sw2.port.onmessage = (e) => {{ globalThis.b = e.data; }}; \
            }})()"
        ),
        "<test>",
    )
    .expect("setup should not throw");

    pump_until(&ctx, |ctx| {
        ctx.eval("globalThis.a !== null && globalThis.b !== null", "<test>")
            .unwrap()
            == "true"
    });

    let a = ctx.eval("globalThis.a", "<test>").unwrap();
    let b = ctx.eval("globalThis.b", "<test>").unwrap();
    assert_eq!(a, "1", "the first connection sees the shared counter at 1");
    assert_eq!(
        b, "2",
        "the second connection must observe the same instance's counter already incremented - proof it's a shared instance, not a fresh one"
    );
}
