//! `Event.composed`/`Event.composedPath()` (closes the last real gap in
//! `spec/matrix/events.md`'s "Capturing phase, once, passive, signal,
//! listener objects and full event path/composed semantics" line — every
//! other item there was already implemented). See
//! `js_runtime::events::event_class`'s own doc on `EventState::path` for
//! the scope cut: only a real `Node`-tree dispatch populates a
//! `composedPath()`; a "simple" target (`window`/`document`) has no DOM
//! tree to walk and always returns `[]`.

use js_runtime::{Context, Runtime};

fn dom_with_nested_elements() -> dom::Dom {
    let mut d = dom::Dom::new();
    let root = d.root();
    let body = d.create_element("body");
    let outer = d.create_element("div");
    d.set_attribute(outer, "id", "outer");
    let inner = d.create_element("span");
    d.set_attribute(inner, "id", "inner");
    d.append_child(root, body);
    d.append_child(body, outer);
    d.append_child(outer, inner);
    d
}

#[test]
fn composed_defaults_to_false_and_reflects_the_constructor_option() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let result = ctx
        .eval(
            "(() => { \
                const a = new Event('x'); \
                const b = new Event('y', { composed: true }); \
                return `${a.composed},${b.composed}`; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "false,true");
}

#[test]
fn composed_path_before_dispatch_is_empty() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let result = ctx
        .eval(
            "(() => { const e = new Event('x'); return e.composedPath().length; })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0");
}

#[test]
fn composed_path_during_dispatch_is_the_real_target_to_root_chain() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom_with_nested_elements());
    let result = ctx
        .eval(
            "(() => { \
                const inner = document.getElementById('inner'); \
                const outer = document.getElementById('outer'); \
                let path = null; \
                inner.addEventListener('poke', (e) => { path = e.composedPath(); }); \
                inner.dispatchEvent(new Event('poke', { bubbles: true })); \
                return [ \
                    path.length >= 2, \
                    path[0] === inner, \
                    path[1] === outer, \
                ]; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,true,true");
}

#[test]
fn composed_path_on_a_simple_target_like_window_is_empty() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());
    let result = ctx
        .eval(
            "(() => { \
                let path = null; \
                window.addEventListener('poke', (e) => { path = e.composedPath(); }); \
                window.dispatchEvent(new Event('poke')); \
                return path.length; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0");
}
