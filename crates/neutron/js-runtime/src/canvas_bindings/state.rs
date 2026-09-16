//! `save`/`restore`/`translate` - the drawing-state stack of
//! `CanvasRenderingContext2D`.
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::context2d_opaque;
use super::helpers::read_js_f32;

pub(super) unsafe extern "C" fn save(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().save();
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn restore(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().restore();
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn translate(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 2 {
        let (x, y) = (read_js_f32(*argv), read_js_f32(*argv.add(1)));
        (*ptr).borrow_mut().translate(x, y);
    }
    sys::js_undefined()
}
