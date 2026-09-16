//! `beginPath`/`moveTo`/`lineTo`/`closePath`/`fill` - the convex-only
//! filled-path surface of `CanvasRenderingContext2D`. See
//! `render::Canvas2D::fill`'s own doc for the exact scope cut.
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::context2d_opaque;
use super::helpers::read_js_f32;

pub(super) unsafe extern "C" fn begin_path(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().begin_path();
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn move_to(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 2 {
        let (x, y) = (read_js_f32(*argv), read_js_f32(*argv.add(1)));
        (*ptr).borrow_mut().move_to(x, y);
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn line_to(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 2 {
        let (x, y) = (read_js_f32(*argv), read_js_f32(*argv.add(1)));
        (*ptr).borrow_mut().line_to(x, y);
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn close_path(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().close_path();
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn fill(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().fill();
    }
    sys::js_undefined()
}
