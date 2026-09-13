//! Shadow DOM (`ROADMAP.md` item 38): `Element.prototype.attachShadow`/
//! `.shadowRoot`, plus `.host`/`.mode` on the shared `Node.prototype`
//! (checked at runtime, same "generic property on the one shared `Node`
//! class" convention this crate already uses for `.value`/`.checked` —
//! see `dom::NodeData::Element`'s own doc). Real DOM structure (a real
//! `dom::NodeData::ShadowRoot` node, `.parent`-linked to its host but
//! excluded from the host's own `children`, matching real light-DOM
//! traversal never seeing a shadow root) — `appendChild`/`querySelector`/
//! `textContent`/etc. all already work on it for free, since this
//! engine's one shared `Node.prototype` (not a per-type hierarchy) has
//! every one of those methods already.
//!
//! Scope cut, deliberate: no render/layout integration (see
//! `dom::NodeData::ShadowRoot`'s own doc), no `<slot>` content
//! projection, and no restriction on which tags can host a shadow root
//! (real spec disallows most; this crate allows any element).

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_class_id_for, node_id, node_object};
use super::util::{new_js_string, read_js_string, throw_type_error};

unsafe fn read_mode(
    ctx: *mut sys::JSContext,
    options: sys::JSValue,
) -> Option<dom::ShadowRootMode> {
    if options.tag != sys::JS_TAG_OBJECT {
        return None;
    }
    let mode_prop = sys::JS_GetPropertyStr(ctx, options, b"mode\0".as_ptr() as *const _);
    let mode = read_js_string(ctx, mode_prop);
    sys::JS_FreeValue(ctx, mode_prop);
    match mode.as_deref() {
        Some("open") => Some(dom::ShadowRootMode::Open),
        Some("closed") => Some(dom::ShadowRootMode::Closed),
        _ => None,
    }
}

unsafe extern "C" fn element_attach_shadow(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(host) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attachShadow target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(host).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    let mode = if argc >= 1 {
        read_mode(ctx, *argv)
    } else {
        None
    };
    let Some(mode) = mode else {
        return throw_type_error(ctx, "attachShadow: options.mode must be 'open' or 'closed'");
    };
    match (*dom).attach_shadow(host, mode) {
        Some(shadow_id) => {
            let class_id = node_class_id_for(ctx, dom, shadow_id);
            node_object(ctx, class_id, shadow_id)
        }
        None => throw_type_error(ctx, "attachShadow: this element already has a shadow root"),
    }
}

unsafe extern "C" fn element_shadow_root_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(host) = node_id(ctx, this_val) else {
        return sys::js_null();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    let Some(shadow_id) = (*dom).shadow_root(host) else {
        return sys::js_null();
    };
    // Real spec: a `closed` shadow root is not reachable via this getter
    // at all — the JS-visible object is only ever handed back from
    // `attachShadow`'s own return value.
    if (*dom).shadow_root_mode(shadow_id) != Some(dom::ShadowRootMode::Open) {
        return sys::js_null();
    }
    let class_id = node_class_id_for(ctx, dom, shadow_id);
    node_object(ctx, class_id, shadow_id)
}

unsafe extern "C" fn node_host_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    match (*dom).shadow_host(id) {
        Some(host) => {
            let class_id = node_class_id_for(ctx, dom, host);
            node_object(ctx, class_id, host)
        }
        None => sys::js_undefined(),
    }
}

unsafe extern "C" fn node_mode_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    match (*dom).shadow_root_mode(id) {
        Some(dom::ShadowRootMode::Open) => new_js_string(ctx, "open"),
        Some(dom::ShadowRootMode::Closed) => new_js_string(ctx, "closed"),
        None => sys::js_undefined(),
    }
}

/// Defines `attachShadow`/`shadowRoot` on `Element.prototype`.
pub(super) unsafe fn define_attach_shadow(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    crate::js_helpers::define_method(ctx, proto, "attachShadow", element_attach_shadow, 1);
    crate::js_helpers::define_getter(ctx, proto, "shadowRoot", element_shadow_root_get);
}

/// Defines `.host`/`.mode` on `Node.prototype` — meaningful only for a
/// `ShadowRoot` node, `undefined` for everything else (checked at
/// runtime, same generic-property convention this file's own doc
/// describes).
pub(super) unsafe fn define_shadow_root_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    crate::js_helpers::define_getter(ctx, proto, "host", node_host_get);
    crate::js_helpers::define_getter(ctx, proto, "mode", node_mode_get);
}
