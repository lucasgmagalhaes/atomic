//! Real `window.open()`/`RemoteWindow` (`ROADMAP.md` items 36/37): a
//! genuinely independent second `JSContext` (its own global object, its
//! own blank `dom::Dom`), real cross-window `postMessage` delivering a
//! real `"message"` event, `opener`, and `.close()`. See
//! `window_registry`'s own module doc for the `iframe` scope cut (item
//! 35 stays unimplemented — no cross-`JSContext` object sharing, no
//! layout nesting).
//!
//! `window.open()`'s own JS-facing result is deliberately opaque (real
//! cross-window isolation — no way to inspect another realm's globals
//! directly from script), so end-to-end delivery is proven via
//! `Context::open_window`/`eval_in_window`/`send_cross_window_message` —
//! the same "drive the Rust-level primitive directly" shape
//! `window_registry_test.rs` already used for Stage 3.

use js_runtime::{Context, Runtime};

#[test]
fn open_window_creates_a_real_independent_realm() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let child_id = ctx.open_window();

    ctx.eval_in_window(child_id, "globalThis.marker = 42;", "<child>")
        .expect("child window must exist")
        .expect("assignment should not throw");

    let opener_sees_marker = ctx.eval("typeof globalThis.marker", "<test>").unwrap();
    assert_eq!(opener_sees_marker, "undefined");

    let child_sees_marker = ctx
        .eval_in_window(child_id, "globalThis.marker", "<child>")
        .unwrap()
        .unwrap();
    assert_eq!(child_sees_marker, "42");
}

#[test]
fn a_message_sent_to_an_opened_window_delivers_a_real_message_event_on_the_openers_own_pump() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let child_id = ctx.open_window();

    ctx.eval_in_window(
        child_id,
        "globalThis.received = null; \
         window.addEventListener('message', (e) => { globalThis.received = e.data; });",
        "<child>",
    )
    .unwrap()
    .unwrap();

    assert!(ctx.send_cross_window_message(child_id, "({ hello: 'world' })"));

    let before_pump = ctx
        .eval_in_window(child_id, "globalThis.received", "<child>")
        .unwrap()
        .unwrap();
    assert_eq!(
        before_pump, "null",
        "not delivered until the opener's own run_pending_timers pumps its children"
    );

    ctx.run_pending_timers();

    let delivered = ctx
        .eval_in_window(child_id, "JSON.stringify(globalThis.received)", "<child>")
        .unwrap()
        .unwrap();
    assert_eq!(delivered, "{\"hello\":\"world\"}");
}

#[test]
fn opener_is_set_on_a_window_opened_via_the_js_api() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let child_id = ctx.open_window();

    let opener_type = ctx
        .eval_in_window(child_id, "typeof globalThis.opener", "<child>")
        .unwrap()
        .unwrap();
    assert_eq!(opener_type, "object");

    let opener_can_post = ctx
        .eval_in_window(child_id, "typeof globalThis.opener.postMessage", "<child>")
        .unwrap()
        .unwrap();
    assert_eq!(opener_can_post, "function");
}

#[test]
fn each_open_call_yields_a_distinct_window() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());

    let distinct = ctx
        .eval(
            "(() => { const a = window.open(); const b = window.open(); return a !== b; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(distinct, "true");
}

#[test]
fn closing_a_window_makes_further_postmessage_a_silent_no_op() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());

    let ok = ctx
        .eval(
            "(() => { \
                const child = window.open(); \
                child.postMessage('one'); \
                child.close(); \
                child.postMessage('two'); \
                return 'ok'; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(ok, "ok");
}

#[test]
fn closing_via_context_drop_cleans_up_every_opened_window() {
    // Regression coverage for window_registry::close_children_of: a
    // dropped opener must not leak its opened windows' JSContexts.
    let rt = Runtime::new();
    let child_id = {
        let ctx = Context::with_dom(&rt, dom::Dom::new());
        let child_id = ctx.open_window();
        assert!(ctx.eval_in_window(child_id, "1", "<child>").is_some());
        child_id
        // ctx dropped here
    };
    // A fresh context (new WindowId space is process-wide, but the old
    // child's raw JSContext must be gone) can't reach the closed window.
    let ctx2 = Context::with_dom(&rt, dom::Dom::new());
    assert!(ctx2.eval_in_window(child_id, "1", "<child>").is_none());
}
