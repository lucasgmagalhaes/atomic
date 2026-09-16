//! `node_list` (`NodeList`-shaped array wrapper) — split out from
//! `collections.rs`.

use std::ffi::CString;

use quickjs_sys as sys;

use super::shared::{collection_item, node_array};

/// Wraps `node_array` into a `NodeList`-like object by adding
/// `item(index)` method on the array.
pub(in super::super) unsafe fn node_list(
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
    array
}
