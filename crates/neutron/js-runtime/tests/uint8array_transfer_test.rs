//! Real `Uint8Array` support in the structured-clone bridge, plus real
//! Transferable-object semantics (`ROADMAP.md` item 32, scoped to
//! `Uint8Array` - see `value_bridge.rs`/`message_channel.rs`'s own docs
//! for why not a bare `ArrayBuffer` or other `TypedArray` kinds).

use js_runtime::{Context, Runtime};

fn to_array_string(ctx: &Context, expr: &str) -> String {
    ctx.eval(&format!("JSON.stringify(Array.from({expr}))"), "<test>")
        .expect("reading the result should not throw")
}

#[test]
fn structured_clone_preserves_a_uint8arrays_real_bytes() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "globalThis.original = new Uint8Array([1, 2, 3, 255]); \
         globalThis.cloned = structuredClone(globalThis.original);",
        "<test>",
    )
    .expect("setup should not throw");

    assert_eq!(
        to_array_string(&ctx, "globalThis.cloned"),
        "[1,2,3,255]",
        "the clone must carry the real source bytes"
    );
}

#[test]
fn structured_clone_of_a_uint8array_is_a_real_independent_copy() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "globalThis.original = new Uint8Array([1, 2, 3]); \
         globalThis.cloned = structuredClone(globalThis.original); \
         globalThis.cloned[0] = 99;",
        "<test>",
    )
    .expect("setup should not throw");

    assert_eq!(
        to_array_string(&ctx, "globalThis.original"),
        "[1,2,3]",
        "mutating the clone must not affect the original"
    );
    assert_eq!(to_array_string(&ctx, "globalThis.cloned"), "[99,2,3]");
}

#[test]
fn a_uint8array_round_trips_through_a_real_message_channel() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.received = null; \
            const channel = new MessageChannel(); \
            channel.port2.onmessage = (e) => { globalThis.received = e.data; }; \
            channel.port1.postMessage(new Uint8Array([7, 8, 9])); \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    ctx.run_pending_timers();

    assert_eq!(
        to_array_string(&ctx, "globalThis.received"),
        "[7,8,9]",
        "the peer must receive a real Uint8Array with the same bytes"
    );
}

#[test]
fn a_transferred_uint8array_is_detached_on_the_senders_own_side() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.received = null; \
            globalThis.sourceLengthAfterSend = null; \
            const channel = new MessageChannel(); \
            channel.port2.onmessage = (e) => { globalThis.received = e.data; }; \
            const buf = new Uint8Array([1, 2, 3]); \
            channel.port1.postMessage(buf, [buf]); \
            globalThis.sourceLengthAfterSend = buf.byteLength; \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    ctx.run_pending_timers();

    assert_eq!(
        ctx.eval("globalThis.sourceLengthAfterSend", "<test>")
            .unwrap(),
        "0",
        "a transferred Uint8Array's source buffer must be detached (byteLength 0)"
    );
    assert_eq!(
        to_array_string(&ctx, "globalThis.received"),
        "[1,2,3]",
        "the peer must still receive the real bytes, captured before the source was detached"
    );
}

#[test]
fn postmessage_without_a_transfer_list_leaves_the_source_untouched() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    ctx.eval(
        "(() => { \
            globalThis.received = null; \
            globalThis.sourceLengthAfterSend = null; \
            const channel = new MessageChannel(); \
            channel.port2.onmessage = (e) => { globalThis.received = e.data; }; \
            const buf = new Uint8Array([4, 5, 6]); \
            channel.port1.postMessage(buf); \
            globalThis.sourceLengthAfterSend = buf.byteLength; \
        })()",
        "<test>",
    )
    .expect("setup should not throw");

    ctx.run_pending_timers();

    assert_eq!(
        ctx.eval("globalThis.sourceLengthAfterSend", "<test>")
            .unwrap(),
        "3",
        "without a transfer list the source Uint8Array must be left alone"
    );
    assert_eq!(to_array_string(&ctx, "globalThis.received"), "[4,5,6]");
}
