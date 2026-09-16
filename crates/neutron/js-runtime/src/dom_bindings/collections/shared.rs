//! `node_array`/`collection_item` — shared by `html_collection`/`node_list`
//! — split out from `collections.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::super::node_registry::{dom_opaque, node_class_id_for, node_object};

/// Wraps each id in `nodes` as a real `Node` object (via the identity
/// cache) into a fresh JS array, in the given order — the shared tail end
/// of `query_selector_all`/`elements_by_tag_name`/`elements_by_class_name`.
pub(super) unsafe fn node_array(ctx: *mut sys::JSContext, nodes: Vec<dom::NodeId>) -> sys::JSValue {
    let array = sys::JS_NewArray(ctx);
    let dom = dom_opaque(ctx);
    for (index, node_id) in nodes.into_iter().enumerate() {
        let class_id = node_class_id_for(ctx, dom, node_id);
        sys::JS_SetPropertyUint32(
            ctx,
            array,
            index as u32,
            node_object(ctx, class_id, node_id),
        );
    }
    array
}

/// `HTMLCollection.prototype.item(index)`/`NodeList.prototype.item(index)`
/// — returns the element at the given index, or `null` if out of range.
pub(super) unsafe extern "C" fn collection_item(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_null();
    }
    let val = *argv;
    if val.tag != sys::JS_TAG_INT {
        return sys::js_null();
    }
    let idx = val.u.int32;
    if idx < 0 {
        return sys::js_null();
    }
    let elem = sys::JS_GetPropertyUint32(ctx, this_val, idx as u32);
    if elem.tag == sys::JS_TAG_UNDEFINED {
        sys::js_null()
    } else {
        elem
    }
}
