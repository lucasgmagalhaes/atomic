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

/// `ctx.arc(x, y, radius, startAngle, endAngle, anticlockwise)` - see
/// `render::Canvas2D::arc`'s own doc: a straight-line polyline
/// approximation, not a true curve primitive. `anticlockwise` defaults to
/// `false` when omitted (matches real spec's own optional 6th argument).
pub(super) unsafe extern "C" fn arc(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 5 {
        let (x, y, radius, start_angle, end_angle) = (
            read_js_f32(*argv),
            read_js_f32(*argv.add(1)),
            read_js_f32(*argv.add(2)),
            read_js_f32(*argv.add(3)),
            read_js_f32(*argv.add(4)),
        );
        let anticlockwise = argc >= 6 && sys::JS_ToBool(ctx, *argv.add(5)) != 0;
        (*ptr)
            .borrow_mut()
            .arc(x, y, radius, start_angle, end_angle, anticlockwise);
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
