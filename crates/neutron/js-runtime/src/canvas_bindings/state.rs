//! `save`/`restore`/`translate`/`scale`/`rotate`/`setTransform`/
//! `resetTransform` - the drawing-state stack of
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

pub(super) unsafe extern "C" fn scale(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 2 {
        let (x, y) = (read_js_f32(*argv), read_js_f32(*argv.add(1)));
        (*ptr).borrow_mut().scale(x, y);
    }
    sys::js_undefined()
}

/// `ctx.rotate(angle)` - `angle` in radians, matching real spec.
pub(super) unsafe extern "C" fn rotate(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 1 {
        let angle = read_js_f32(*argv);
        (*ptr).borrow_mut().rotate(angle);
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn set_transform(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 6 {
        let (a, b, c, d, e, f) = (
            read_js_f32(*argv),
            read_js_f32(*argv.add(1)),
            read_js_f32(*argv.add(2)),
            read_js_f32(*argv.add(3)),
            read_js_f32(*argv.add(4)),
            read_js_f32(*argv.add(5)),
        );
        (*ptr).borrow_mut().set_transform(a, b, c, d, e, f);
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn reset_transform(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        (*ptr).borrow_mut().reset_transform();
    }
    sys::js_undefined()
}
