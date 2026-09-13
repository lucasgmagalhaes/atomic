//! Real `Range`/`Selection` (`ROADMAP.md` item 20, "text selection") —
//! script-driven construction only. See `js_runtime::selection`'s own
//! module doc for the full scope note (no user-driven mouse-drag/keyboard
//! selection, text-node boundary containers only).

use js_runtime::{Context, Runtime};

#[test]
fn a_range_over_a_single_text_node_returns_the_real_substring() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("hello world");
    d.append_child(root, p);
    d.append_child(p, text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const range = document.createRange(); \
                const node = document.querySelector('p').firstChild; \
                range.setStart(node, 0); \
                range.setEnd(node, 5); \
                return range.toString(); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "hello");
}

#[test]
fn a_range_spanning_two_sibling_text_nodes_concatenates_real_text() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let first = d.create_text("hello ");
    let b = d.create_element("b");
    let second = d.create_text("world");
    d.append_child(root, p);
    d.append_child(p, first);
    d.append_child(p, b);
    d.append_child(b, second);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const range = document.createRange(); \
                const p = document.querySelector('p'); \
                const first = p.firstChild; \
                const second = p.querySelector('b').firstChild; \
                range.setStart(first, 0); \
                range.setEnd(second, 5); \
                return range.toString(); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "hello world");
}

#[test]
fn collapsed_is_true_only_when_start_equals_end() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("hello");
    d.append_child(root, p);
    d.append_child(p, text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const range = document.createRange(); \
                const node = document.querySelector('p').firstChild; \
                range.setStart(node, 2); \
                range.setEnd(node, 2); \
                const wasCollapsed = range.collapsed; \
                range.setEnd(node, 4); \
                return `${wasCollapsed},${range.collapsed}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,false");
}

#[test]
fn selection_addrange_and_tostring_delegate_to_the_stored_range() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("hello world");
    d.append_child(root, p);
    d.append_child(p, text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const node = document.querySelector('p').firstChild; \
                const range = document.createRange(); \
                range.setStart(node, 0); \
                range.setEnd(node, 5); \
                const sel = window.getSelection(); \
                sel.addRange(range); \
                return `${sel.rangeCount},${sel.toString()}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "1,hello");
}

#[test]
fn removeallranges_resets_rangecount_and_tostring() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let text = d.create_text("hello");
    d.append_child(root, text);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const sel = window.getSelection(); \
                const range = document.createRange(); \
                sel.addRange(range); \
                sel.removeAllRanges(); \
                return `${sel.rangeCount},${sel.toString()}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,");
}

#[test]
fn getselection_returns_the_same_object_every_call() {
    let rt = Runtime::new();
    let ctx = Context::new(&rt);

    let result = ctx
        .eval("window.getSelection() === window.getSelection()", "<test>")
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn window_and_document_getselection_return_the_same_object() {
    let d = dom::Dom::new();
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "window.getSelection() === document.getSelection()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn an_element_node_boundary_is_a_documented_no_op() {
    let mut d = dom::Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);

    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, d);

    // Setting a boundary on an element (not a text node) must not throw,
    // and the range stays collapsed at its default (document root)
    // position - a documented no-op, not a spec-shaped exception.
    let result = ctx
        .eval(
            "(() => { \
                const range = document.createRange(); \
                const el = document.querySelector('p'); \
                range.setStart(el, 0); \
                return typeof range.toString(); \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "string");
}
