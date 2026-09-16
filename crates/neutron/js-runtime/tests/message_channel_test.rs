use js_runtime::{Context, Runtime};

#[test]
fn port2_receives_a_structured_clone_via_onmessage() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.received = null; \
            const channel = new MessageChannel(); \
            globalThis.port1 = channel.port1; \
            channel.port2.onmessage = (e) => { globalThis.received = e.data; }; \
            channel.port1.postMessage({ a: 1, b: 'x' }); \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    ctx.run_pending_timers();

    let received = ctx
        .eval("JSON.stringify(globalThis.received)", "<test>")
        .expect("reading the result should not throw");
    assert_eq!(
        received, "{\"a\":1,\"b\":\"x\"}",
        "port2 must receive a real structured clone of the object posted on port1"
    );
}

#[test]
fn posted_data_is_a_real_independent_clone_not_a_shared_reference() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.received = null; \
            const channel = new MessageChannel(); \
            channel.port2.onmessage = (e) => { globalThis.received = e.data; }; \
            const obj = { count: 1 }; \
            channel.port1.postMessage(obj); \
            obj.count = 999; \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    ctx.run_pending_timers();

    let count = ctx
        .eval("globalThis.received.count", "<test>")
        .expect("reading the count should not throw");
    assert_eq!(
        count, "1",
        "postMessage must clone data immediately, not keep a live reference to the original object"
    );
}

#[test]
fn add_event_listener_and_onmessage_both_receive_the_message() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.log = []; \
            const channel = new MessageChannel(); \
            channel.port2.addEventListener('message', (e) => { globalThis.log.push('listener:' + e.data); }); \
            channel.port2.onmessage = (e) => { globalThis.log.push('onmessage:' + e.data); }; \
            channel.port1.postMessage('hi'); \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    ctx.run_pending_timers();

    let log = ctx
        .eval("globalThis.log.slice().sort().join(',')", "<test>")
        .expect("reading the log should not throw");
    assert_eq!(
        log, "listener:hi,onmessage:hi",
        "both addEventListener('message', ...) and the onmessage IDL attribute must fire"
    );
}

#[test]
fn a_closed_port_drops_further_messages() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.count = 0; \
            globalThis.channel = new MessageChannel(); \
            globalThis.channel.port2.onmessage = () => { globalThis.count++; }; \
            globalThis.channel.port2.close(); \
            globalThis.channel.port1.postMessage('one'); \
        })()",
        "<test>",
    )
    .expect("setup should not throw");
    ctx.run_pending_timers();

    let count = ctx
        .eval("globalThis.count", "<test>")
        .expect("reading the count should not throw");
    assert_eq!(
        count, "0",
        "a message posted to a closed port must be dropped, not delivered"
    );
}
