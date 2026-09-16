//! Window/`BrowsingContext` identity round-trip (`ROADMAP.md` items
//! 35-37, `.claude/plans/architecture-p5-foundations.plan.md` Stage 3):
//! two independent `Context`s, each with its own window identity, can
//! look each other up and exchange a cross-context message — the actual
//! mechanism `iframe`/`window.open`/`window.postMessage` will each build
//! on, exercised here without any of those JS-facing APIs existing yet.

use js_runtime::{Context, Runtime};

#[test]
fn two_contexts_get_distinct_window_ids() {
    let rt1 = Runtime::new();
    let ctx1 = Context::with_dom(&rt1, dom::Dom::new());
    let rt2 = Runtime::new();
    let ctx2 = Context::with_dom(&rt2, dom::Dom::new());

    let id1 = ctx1.window_id().expect("with_dom must assign a window id");
    let id2 = ctx2.window_id().expect("with_dom must assign a window id");
    assert_ne!(id1, id2);
}

#[test]
fn a_plain_context_without_dom_has_no_window_id() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);
    assert!(ctx.window_id().is_none());
}

#[test]
fn a_message_sent_to_another_window_is_delivered_on_its_own_pump() {
    let rt1 = Runtime::new();
    let ctx1 = Context::with_dom(&rt1, dom::Dom::new());
    let rt2 = Runtime::new();
    let ctx2 = Context::with_dom(&rt2, dom::Dom::new());
    let id2 = ctx2.window_id().unwrap();

    ctx2.eval(
        "globalThis.received = null; \
         window.addEventListener('message', (e) => { globalThis.received = e.data; });",
        "<test>",
    )
    .unwrap();

    let sent = ctx1.send_cross_window_message(id2, "({ a: 1, b: 'x' })");
    assert!(sent, "sending to a live window must succeed");

    // Not delivered until ctx2 pumps — matches every other "no real event
    // loop, a host pumps explicitly" primitive in this crate.
    let before_pump = ctx2.eval("globalThis.received", "<test>").unwrap();
    assert_eq!(before_pump, "null");

    ctx2.run_pending_timers();

    // Real "message" event delivery (ROADMAP.md item 37's own JS-facing
    // completion, see window_registry.rs's own doc) — not the internal
    // test-only array Stage 3 originally used.
    let delivered = ctx2
        .eval("JSON.stringify(globalThis.received)", "<test>")
        .expect("reading the received value should not throw");
    assert_eq!(delivered, "{\"a\":1,\"b\":\"x\"}");
}

#[test]
fn sending_to_a_window_that_no_longer_exists_fails_and_never_panics() {
    let rt1 = Runtime::new();
    let ctx1 = Context::with_dom(&rt1, dom::Dom::new());

    let closed_id = {
        let rt2 = Runtime::new();
        let ctx2 = Context::with_dom(&rt2, dom::Dom::new());
        ctx2.window_id().unwrap()
        // ctx2 dropped here, unregistering its window id
    };

    let sent = ctx1.send_cross_window_message(closed_id, "'hello'");
    assert!(
        !sent,
        "a message to a closed/nonexistent window must be dropped, not delivered"
    );
}
