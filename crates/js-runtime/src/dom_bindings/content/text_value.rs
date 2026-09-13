//! `textContent`/`value`/`nodeValue` accessors on `Node.prototype` — split
//! out from `content/mod.rs`.

use quickjs_sys as sys;

use crate::js_helpers::define_getter_setter;

use super::super::node_registry::{dom_opaque, node_opaque};
use super::super::util::{new_js_string, read_js_string};

unsafe extern "C" fn node_text_content_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if node_ptr.is_null() || dom_ptr.is_null() {
        return sys::js_undefined();
    }
    new_js_string(ctx, &(*dom_ptr).text_content(*node_ptr))
}

unsafe extern "C" fn node_text_content_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    let Some(text) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        (*dom_ptr).set_text_content(*node_ptr, &text);
    }
    sys::js_undefined()
}

/// Defines the `textContent` accessor on `proto`. Takes ownership of `proto`
/// only in the sense of mutating it in place — callers still own the value.
pub(in super::super) unsafe fn define_text_content(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    define_getter_setter(
        ctx,
        proto,
        "textContent",
        node_text_content_get,
        node_text_content_set,
    );
}

unsafe extern "C" fn node_value_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if node_ptr.is_null() || dom_ptr.is_null() {
        return sys::js_undefined();
    }
    new_js_string(ctx, &(*dom_ptr).value(*node_ptr))
}

unsafe extern "C" fn node_value_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    let Some(text) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        (*dom_ptr).set_value(*node_ptr, &text);
        // Real `"input"` semantics: fires on every value mutation,
        // regardless of whether it came from a user keystroke
        // (`profile-worker`'s `type_key`) or a script setting `.value =`
        // directly — matches the real DOM, which doesn't distinguish the
        // two for this event.
        crate::events::dispatch(ctx, this_val, "input");
    }
    sys::js_undefined()
}

/// Defines the `value` accessor on `proto` — real, independent of
/// `textContent` (see `dom::Dom::value`/`set_value`'s own docs for the
/// deviation from a typed `HTMLInputElement`/`HTMLTextAreaElement`
/// hierarchy this generic `Node` class makes).
pub(in super::super) unsafe fn define_value(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    define_getter_setter(ctx, proto, "value", node_value_get, node_value_set);
}

/// Real `Node.prototype.nodeValue` getter — per spec:
/// - Text / Comment nodes: the text data
/// - Document / DocumentFragment / Element: `null`
unsafe extern "C" fn node_node_value_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if node_ptr.is_null() || dom_ptr.is_null() {
        return sys::js_undefined();
    }
    match (*dom_ptr).node_value(*node_ptr) {
        Some(text) => new_js_string(ctx, &text),
        None => sys::js_null(),
    }
}

/// Real `Node.prototype.nodeValue` setter — updates text for Text/Comment
/// nodes, no-op for everything else (Element, Document, DocumentFragment).
unsafe extern "C" fn node_node_value_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    let Some(text) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        (*dom_ptr).set_node_value(*node_ptr, &text);
    }
    sys::js_undefined()
}

/// Defines the `nodeValue` accessor on `proto` — per spec, returns the
/// text data for Text/Comment nodes, `null` for everything else.
pub(in super::super) unsafe fn define_node_value(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    define_getter_setter(
        ctx,
        proto,
        "nodeValue",
        node_node_value_get,
        node_node_value_set,
    );
}
