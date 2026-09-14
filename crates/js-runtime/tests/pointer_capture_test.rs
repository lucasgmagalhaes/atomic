//! Real `Element.setPointerCapture`/`releasePointerCapture`/
//! `hasPointerCapture` and their `"gotpointercapture"`/
//! `"lostpointercapture"` events - closes the "pointer capture" part of
//! `spec/matrix/events.md` line 17. Scope cut (see
//! `dom_bindings::pointer_capture`'s own doc): this engine dispatches no
//! real host-driven `PointerEvent`s to redirect through a captured
//! target - what's real here is the bookkeeping API and its events.

use js_runtime::{Context, Runtime};

fn dom_with_element() -> dom::Dom {
    let mut d = dom::Dom::new();
    let root = d.root();
    let el = d.create_element("div");
    d.set_attribute(el, "id", "el");
    d.append_child(root, el);
    d
}

#[test]
fn set_pointer_capture_makes_has_pointer_capture_true_and_fires_gotpointercapture() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_element());
    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('el'); \
                let got = false; \
                el.addEventListener('gotpointercapture', () => { got = true; }); \
                el.setPointerCapture(1); \
                return `${el.hasPointerCapture(1)},${got}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true");
}

#[test]
fn has_pointer_capture_is_false_for_an_uncaptured_pointer_id() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_element());
    let result = ctx
        .eval(
            "(() => { const el = document.getElementById('el'); return el.hasPointerCapture(99); })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false");
}

#[test]
fn release_pointer_capture_clears_capture_and_fires_lostpointercapture() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_element());
    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('el'); \
                let lost = false; \
                el.addEventListener('lostpointercapture', () => { lost = true; }); \
                el.setPointerCapture(2); \
                el.releasePointerCapture(2); \
                return `${el.hasPointerCapture(2)},${lost}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false,true");
}

#[test]
fn releasing_a_pointer_that_was_never_captured_is_a_real_no_op() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_element());
    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('el'); \
                let lost = false; \
                el.addEventListener('lostpointercapture', () => { lost = true; }); \
                el.releasePointerCapture(5); \
                return lost; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false");
}

#[test]
fn setting_pointer_capture_again_on_a_different_element_moves_it() {
    let rt = Runtime::new();
    let mut d = dom_with_element();
    let root = d.root();
    let other = d.create_element("span");
    d.set_attribute(other, "id", "other");
    d.append_child(root, other);
    let ctx = Context::with_dom(&rt, d);
    let result = ctx
        .eval(
            "(() => { \
                const el = document.getElementById('el'); \
                const other = document.getElementById('other'); \
                el.setPointerCapture(1); \
                other.setPointerCapture(1); \
                return `${el.hasPointerCapture(1)},${other.hasPointerCapture(1)}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false,true");
}
