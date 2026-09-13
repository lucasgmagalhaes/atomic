//! No-op `instanceof`-only constructors and object construction helpers,
//! split out from `element_classes.rs`.

use std::ffi::CString;
use std::os::raw::c_void;

use quickjs_sys as sys;

/// Exposes a global `Name` constructor whose `.prototype` is `proto`. Also
/// used by `document.rs` for the `DOMParser`/`XMLSerializer` globals.
pub(in crate::dom_bindings) unsafe fn expose_constructor(
    ctx: *mut sys::JSContext,
    name: &str,
    constructor_fn: sys::JSCFunction,
    proto: sys::JSValue,
) {
    let cname = CString::new(name).unwrap();
    let constructor = sys::JS_NewCFunction2(
        ctx,
        constructor_fn,
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetConstructorBit(ctx, constructor, true);
    let proto_for_prop = sys::JS_DupValue(ctx, proto);
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(ctx, constructor, proto_name.as_ptr(), proto_for_prop);
    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, cname.as_ptr(), constructor);
    sys::JS_FreeValue(ctx, global);
}

/// Minimal no-op constructor for the interface hierarchy.
/// These exist solely to provide `instanceof` support — the real node
/// creation always goes through `make_node_object` with the correct class_id.
pub(super) unsafe extern "C" fn node_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

pub(super) unsafe extern "C" fn element_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

pub(super) unsafe extern "C" fn html_element_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

pub(super) unsafe extern "C" fn html_input_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

pub(super) unsafe extern "C" fn html_button_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

pub(super) unsafe extern "C" fn html_anchor_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

pub(super) unsafe extern "C" fn html_image_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

pub(super) unsafe extern "C" fn html_canvas_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

pub(super) unsafe extern "C" fn html_form_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

pub(super) unsafe extern "C" fn html_select_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

pub(super) unsafe extern "C" fn html_iframe_constructor(
    _ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    this_val
}

/// Also used by `node_registry::node_object` to build the actual JS object
/// for a real `dom::NodeId` once identity-cache lookup misses.
pub(in crate::dom_bindings) unsafe fn make_node_object(
    ctx: *mut sys::JSContext,
    class_id: sys::JSClassID,
    id: dom::NodeId,
) -> sys::JSValue {
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    sys::JS_SetOpaque(obj, Box::into_raw(Box::new(id)) as *mut c_void);
    obj
}
