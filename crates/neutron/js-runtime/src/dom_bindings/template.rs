//! `<template>` (`ROADMAP.md`'s "Default actions" neighbor item, closing
//! `spec/matrix/dom.md`'s last-named custom-elements/shadow-DOM cluster
//! gap): real `.content`, a genuinely isolated `DocumentFragment` — not
//! visible to the template's own `childNodes`/light-DOM traversal.
//!
//! The isolation itself is real work done in `dom`/`html`, not here:
//! `dom::Dom::create_element` eagerly creates the content fragment for
//! every `"template"`-tagged element, and `html::Sink::get_template_contents`
//! redirects `html5ever`'s own insertions under a `<template>` into it.
//! This module is just the thin JS-facing read side — `.content` wraps
//! that already-real fragment `NodeId` the same way `document_creation`'s
//! `document_create_document_fragment` wraps a fresh one.
//!
//! Scope cut: `cloneNode`/`importNode` on a `<template>` don't clone its
//! content fragment (same "not cloned" treatment `dom::NodeData::Element`'s
//! own doc gives `shadow_root`) — no template-instantiation consumer
//! exists anywhere in this engine.

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_class_id_for, node_id, node_object};

unsafe extern "C" fn template_content_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_null();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    let Some(content_id) = (*dom).template_content(id) else {
        return sys::js_null();
    };
    let class_id = node_class_id_for(ctx, dom, content_id);
    node_object(ctx, class_id, content_id)
}

/// Defines `.content` (read-only) on `HTMLTemplateElement.prototype`.
pub(super) unsafe fn define_template_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    crate::js_helpers::define_getter(ctx, proto, "content", template_content_get);
}
