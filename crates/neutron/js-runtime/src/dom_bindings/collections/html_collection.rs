//! `html_collection` (`HTMLCollection`-shaped array wrapper) — split out
//! from `collections.rs`.

use std::ffi::CString;

use quickjs_sys as sys;

use super::super::util::read_js_string;
use super::shared::{collection_item, node_array};

/// Wraps `node_array` into an `HTMLCollection`-like object by adding
/// `item(index)` and `namedItem(name)` methods on the array.
/// `namedItem` matches by `id` or `name` attribute.
pub(in super::super) unsafe fn html_collection(
    ctx: *mut sys::JSContext,
    nodes: Vec<dom::NodeId>,
) -> sys::JSValue {
    let array = node_array(ctx, nodes);
    let item_name = CString::new("item").unwrap();
    let item_fn = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<sys::JSCFunction, sys::JSCFunction>(collection_item),
        item_name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, array, item_name.as_ptr(), item_fn);
    let named_name = CString::new("namedItem").unwrap();
    let named_fn = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<sys::JSCFunction, sys::JSCFunction>(collection_named_item),
        named_name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, array, named_name.as_ptr(), named_fn);
    array
}

/// `HTMLCollection.prototype.namedItem(name)` — returns the first element
/// whose `id` or `name` attribute matches `name`, or `null` if none.
unsafe extern "C" fn collection_named_item(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: std::os::raw::c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_null();
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return sys::js_null();
    };
    if name.is_empty() {
        return sys::js_null();
    }
    let length_prop = CString::new("length").unwrap();
    let length = sys::JS_GetPropertyStr(ctx, this_val, length_prop.as_ptr());
    let len = if length.tag == sys::JS_TAG_INT {
        length.u.int32
    } else {
        0
    };
    sys::JS_FreeValue(ctx, length);
    for i in 0..len {
        let elem = sys::JS_GetPropertyUint32(ctx, this_val, i as u32);
        if elem.tag == sys::JS_TAG_UNDEFINED || elem.tag == sys::JS_TAG_NULL {
            continue;
        }
        if element_matches_name(ctx, elem, &name) {
            return elem;
        }
        sys::JS_FreeValue(ctx, elem);
    }
    sys::js_null()
}

/// Checks if an element's `id` or `name` attribute matches the given name.
unsafe fn element_matches_name(ctx: *mut sys::JSContext, elem: sys::JSValue, name: &str) -> bool {
    let id_atom = CString::new("id").unwrap();
    let id_val = sys::JS_GetPropertyStr(ctx, elem, id_atom.as_ptr());
    let matched = if let Some(id_str) = read_js_string(ctx, id_val) {
        id_str == name
    } else {
        false
    };
    sys::JS_FreeValue(ctx, id_val);
    if matched {
        return true;
    }
    let name_atom = CString::new("name").unwrap();
    let name_val = sys::JS_GetPropertyStr(ctx, elem, name_atom.as_ptr());
    let matched = if let Some(n) = read_js_string(ctx, name_val) {
        n == name
    } else {
        false
    };
    sys::JS_FreeValue(ctx, name_val);
    matched
}
