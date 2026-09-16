//! `fillRect`/`clearRect`/`strokeRect` and the `fillStyle`/`strokeStyle`/
//! `lineWidth` getter/setters - the plain-rectangle drawing surface of
//! `CanvasRenderingContext2D`.
use std::cell::RefCell;
use std::os::raw::c_int;

use quickjs_sys as sys;

use layout_engine::Color;

use render::Pattern;

use super::context2d_opaque;
use super::gradient::{resolve_gradient, GradientData, GRADIENT_CLASS_KIND};
use super::helpers::{format_hex_color, parse_hex_color, read_js_f32, read_js_string};
use super::pattern::pattern_opaque;

pub(super) unsafe extern "C" fn fill_rect(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 4 {
        let (x, y, w, h) = (
            read_js_f32(*argv),
            read_js_f32(*argv.add(1)),
            read_js_f32(*argv.add(2)),
            read_js_f32(*argv.add(3)),
        );
        (*ptr).borrow_mut().fill_rect(x, y, w, h);
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn clear_rect(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 4 {
        let (x, y, w, h) = (
            read_js_f32(*argv),
            read_js_f32(*argv.add(1)),
            read_js_f32(*argv.add(2)),
            read_js_f32(*argv.add(3)),
        );
        (*ptr).borrow_mut().clear_rect(x, y, w, h);
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn stroke_rect(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 4 {
        let (x, y, w, h) = (
            read_js_f32(*argv),
            read_js_f32(*argv.add(1)),
            read_js_f32(*argv.add(2)),
            read_js_f32(*argv.add(3)),
        );
        (*ptr).borrow_mut().stroke_rect(x, y, w, h);
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn fill_style_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    let color = if ptr.is_null() {
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    } else {
        (*ptr).borrow().fill_style()
    };
    let s = format_hex_color(color);
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

pub(super) unsafe extern "C" fn fill_style_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    // A `CanvasGradient`/`CanvasPattern` (from `createLinearGradient`/
    // `createRadialGradient`/`createConicGradient`/`createPattern`) takes
    // priority over treating `val` as a hex string - real spec assigns
    // whatever value's own type dictates (string vs object), this crate
    // just checks the two object classes it actually has.
    if val.tag == sys::JS_TAG_OBJECT {
        let rt = sys::JS_GetRuntime(ctx);
        let class_id = crate::class_registry::class_id_for(rt, GRADIENT_CLASS_KIND);
        let gptr = sys::JS_GetOpaque(val, class_id) as *mut RefCell<GradientData>;
        if !gptr.is_null() {
            if let Some(gradient) = resolve_gradient(&(*gptr).borrow()) {
                (*ptr).borrow_mut().set_fill_gradient(gradient);
            }
            return sys::js_undefined();
        }
        let pptr = pattern_opaque(rt, val);
        if !pptr.is_null() {
            let pattern: Pattern = (*pptr).borrow().clone();
            (*ptr).borrow_mut().set_fill_pattern(pattern);
            return sys::js_undefined();
        }
    }
    if let Some(s) = read_js_string(ctx, val) {
        if let Some(color) = parse_hex_color(s.trim()) {
            (*ptr).borrow_mut().set_fill_style(color);
        }
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn stroke_style_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    let color = if ptr.is_null() {
        Color {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    } else {
        (*ptr).borrow().stroke_style()
    };
    let s = format_hex_color(color);
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

pub(super) unsafe extern "C" fn stroke_style_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            if let Some(color) = parse_hex_color(s.trim()) {
                (*ptr).borrow_mut().set_stroke_style(color);
            }
        }
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn line_width_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    let width = if ptr.is_null() {
        1.0
    } else {
        (*ptr).borrow().line_width()
    };
    sys::js_float64(width as f64)
}

pub(super) unsafe extern "C" fn line_width_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        let width = read_js_f32(val);
        if width > 0.0 {
            (*ptr).borrow_mut().set_line_width(width);
        }
    }
    sys::js_undefined()
}
