//! `<template>` real `.content` (closes `spec/matrix/dom.md`'s last-named
//! custom-elements/shadow-DOM cluster gap): a genuinely isolated
//! `DocumentFragment`, not visible via the template's own `childNodes` —
//! see `js_runtime::dom_bindings::template`'s own module doc for the
//! scope cut (no `cloneNode` content cloning).

use js_runtime::{Context, Runtime};

#[test]
fn created_via_document_create_element_has_a_real_empty_content_fragment() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());

    let result = ctx
        .eval(
            "(() => { \
                const t = document.createElement('template'); \
                return [ \
                    t.content !== null, \
                    t.content.nodeType, \
                    t.content.childNodes.length, \
                    t.childNodes.length, \
                ]; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true,11,0,0");
}

#[test]
fn parsed_template_children_live_in_content_not_light_dom() {
    let rt = Runtime::new();
    let d = html::parse("<template><p>hi</p></template>");
    let ctx = Context::with_dom(&rt, d);

    let result = ctx
        .eval(
            "(() => { \
                const t = document.querySelector('template'); \
                return [ \
                    t.childNodes.length, \
                    t.content.childNodes.length, \
                    t.content.querySelector('p').textContent, \
                ]; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,1,hi");
}

#[test]
fn content_is_the_same_object_across_repeated_accesses() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());

    let result = ctx
        .eval(
            "(() => { \
                const t = document.createElement('template'); \
                return t.content === t.content; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "true");
}

#[test]
fn appending_into_content_does_not_affect_the_templates_own_children() {
    let rt = Runtime::new();
    let ctx = Context::with_dom(&rt, dom::Dom::new());

    let result = ctx
        .eval(
            "(() => { \
                const t = document.createElement('template'); \
                t.content.appendChild(document.createElement('span')); \
                return [t.childNodes.length, t.content.childNodes.length]; \
            })()",
            "<test>",
        )
        .unwrap();
    assert_eq!(result, "0,1");
}
