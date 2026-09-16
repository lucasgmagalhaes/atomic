//! `<iframe>.contentWindow` (`ROADMAP.md` item 35): a real, independent
//! child browsing context, opened lazily on first access — same
//! primitive `window.open()` (items 36/37) already uses. See
//! `dom_bindings::iframe`'s own module doc for the scope cut: no
//! `.contentDocument`, no visual embedding.

use js_runtime::{Context, Runtime};

fn dom_with_iframe() -> (dom::Dom, dom::NodeId) {
    let mut d = dom::Dom::new();
    let root = d.root();
    let iframe = d.create_element("iframe");
    d.append_child(root, iframe);
    (d, iframe)
}

#[test]
fn content_window_is_a_real_object_with_postmessage() {
    let rt = Runtime::new();
    let (dom, _iframe_id) = dom_with_iframe();
    let ctx = Context::with_dom(&rt, dom);

    let is_function = ctx
        .eval(
            "(() => { \
                const frame = document.querySelector('iframe'); \
                return typeof frame.contentWindow.postMessage; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(is_function, "function");
}

#[test]
fn repeated_access_returns_the_same_window_not_a_fresh_one() {
    let rt = Runtime::new();
    let (dom, _iframe_id) = dom_with_iframe();
    let ctx = Context::with_dom(&rt, dom);

    let same = ctx
        .eval(
            "(() => { \
                const frame = document.querySelector('iframe'); \
                return frame.contentWindow === frame.contentWindow; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(same, "true");
}

#[test]
fn two_different_iframes_get_two_different_windows() {
    let rt = Runtime::new();
    let mut d = dom::Dom::new();
    let root = d.root();
    let a = d.create_element("iframe");
    let b = d.create_element("iframe");
    d.append_child(root, a);
    d.append_child(root, b);
    let ctx = Context::with_dom(&rt, d);

    let distinct = ctx
        .eval(
            "(() => { \
                const frames = document.querySelectorAll('iframe'); \
                return frames[0].contentWindow === frames[1].contentWindow; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(distinct, "false");
}

#[test]
fn content_window_has_no_content_document_by_design() {
    let rt = Runtime::new();
    let (dom, _iframe_id) = dom_with_iframe();
    let ctx = Context::with_dom(&rt, dom);

    let content_document = ctx
        .eval(
            "(() => { \
                const frame = document.querySelector('iframe'); \
                return typeof frame.contentDocument; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(content_document, "undefined");
}

#[test]
fn a_message_posted_to_contentwindow_delivers_a_real_message_event_in_the_child() {
    let rt = Runtime::new();
    let (dom, iframe_id) = dom_with_iframe();
    let ctx = Context::with_dom(&rt, dom);

    // Resolve the same child window the JS-facing getter will hand back,
    // via the Rust-level entry point, so we can install a listener and
    // read the result back directly in its own realm.
    let window_id = ctx.window_id_for_iframe(iframe_id);
    ctx.eval_in_window(
        window_id,
        "globalThis.received = null; \
         window.addEventListener('message', (e) => { globalThis.received = e.data; });",
        "<child>",
    )
    .unwrap()
    .unwrap();

    ctx.eval(
        "document.querySelector('iframe').contentWindow.postMessage({ a: 1 });",
        "<test>",
    )
    .unwrap();

    let before_pump = ctx
        .eval_in_window(window_id, "globalThis.received", "<child>")
        .unwrap()
        .unwrap();
    assert_eq!(before_pump, "null");

    ctx.run_pending_timers();

    let delivered = ctx
        .eval_in_window(window_id, "JSON.stringify(globalThis.received)", "<child>")
        .unwrap()
        .unwrap();
    assert_eq!(delivered, "{\"a\":1}");
}
