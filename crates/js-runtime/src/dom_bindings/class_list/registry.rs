//! The `CLASS_LIST_OBJECTS` identity cache, `cleanup`, and
//! `sync_class_list` — split out from `class_list.rs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;

use quickjs_sys as sys;

use super::tokens::class_tokens;

thread_local! {
    pub(super) static CLASS_LIST_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
    /// Last synced token count per `(ctx, NodeId)` classList object, so
    /// `sync_class_list` knows how many trailing indexed properties (from a
    /// shrunk class attribute) need clearing to `undefined` rather than left
    /// stale from a longer previous token list.
    static CLASS_LIST_LENGTHS: RefCell<HashMap<usize, HashMap<dom::NodeId, usize>>> = RefCell::new(HashMap::new());
}

/// Frees every cached `classList` object for `ctx` — called from
/// `node_registry::cleanup`.
pub(in super::super) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    crate::js_helpers::cleanup_object_cache(&CLASS_LIST_OBJECTS, ctx);
    crate::js_helpers::cleanup_aux_map(&CLASS_LIST_LENGTHS, ctx);
}

/// Refreshes a classList object's indexed properties (`0`, `1`, ...) and
/// `length` from the node's live `class` attribute, clearing any trailing
/// index left over from a longer previous token list. Called on every
/// mutation and every `element.classList` getter hit, so a cached wrapper
/// (identity-stable per `NodeId`, see `CLASS_LIST_OBJECTS`) never serves
/// stale indices after a direct `setAttribute("class", ...)` bypassed it.
pub(super) unsafe fn sync_class_list(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
    object: sys::JSValue,
) {
    let tokens = class_tokens((*dom).attribute(id, "class").unwrap_or_default());
    let old_len = CLASS_LIST_LENGTHS
        .with(|reg| {
            reg.borrow()
                .get(&(ctx as usize))
                .and_then(|nodes| nodes.get(&id).copied())
        })
        .unwrap_or(0);
    for (index, token) in tokens.iter().enumerate() {
        sys::JS_SetPropertyUint32(
            ctx,
            object,
            index as u32,
            crate::dom_bindings::util::new_js_string(ctx, token),
        );
    }
    for index in tokens.len()..old_len {
        sys::JS_SetPropertyUint32(ctx, object, index as u32, sys::js_undefined());
    }
    let length_name = CString::new("length").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        object,
        length_name.as_ptr(),
        sys::js_float64(tokens.len() as f64),
    );
    CLASS_LIST_LENGTHS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, tokens.len())
    });
}
