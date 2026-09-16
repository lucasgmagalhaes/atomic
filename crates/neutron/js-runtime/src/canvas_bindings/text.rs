//! `fillText`/`strokeText`/`measureText` and the `font` getter/setter -
//! the text surface of `CanvasRenderingContext2D`.
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::context2d_opaque;
use super::helpers::{read_js_f32, read_js_string, set_prop_f64};

/// `ctx.fillText(text, x, y)` — the `maxWidth` 4th argument isn't
/// accepted (see `render::Canvas2D::fill_text`'s own doc for this and its
/// other scope cuts).
pub(super) unsafe extern "C" fn fill_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 3 {
        if let Some(text) = read_js_string(ctx, *argv) {
            let x = read_js_f32(*argv.add(1));
            let y = read_js_f32(*argv.add(2));
            (*ptr).borrow_mut().fill_text(&text, x, y);
        }
    }
    sys::js_undefined()
}

/// `ctx.strokeText(text, x, y)` — see `render::Canvas2D::stroke_text`'s
/// own doc: not a real outline stroke, paints the same glyphs solid in
/// `strokeStyle`.
pub(super) unsafe extern "C" fn stroke_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() && argc >= 3 {
        if let Some(text) = read_js_string(ctx, *argv) {
            let x = read_js_f32(*argv.add(1));
            let y = read_js_f32(*argv.add(2));
            (*ptr).borrow_mut().stroke_text(&text, x, y);
        }
    }
    sys::js_undefined()
}

/// `ctx.measureText(text)` — returns a plain object with just `width`
/// (see `render::Canvas2D::measure_text`'s own doc for why no other
/// `TextMetrics` fields exist).
pub(super) unsafe extern "C" fn measure_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    let width = if ptr.is_null() || argc < 1 {
        0.0
    } else {
        match read_js_string(ctx, *argv) {
            Some(text) => (*ptr).borrow().measure_text(&text),
            None => 0.0,
        }
    };
    let obj = sys::JS_NewObject(ctx);
    set_prop_f64(ctx, obj, "width", width as f64);
    obj
}

pub(super) unsafe extern "C" fn font_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    let s = if ptr.is_null() {
        "16px sans-serif".to_string()
    } else {
        (*ptr).borrow().font()
    };
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

pub(super) unsafe extern "C" fn font_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let ptr = context2d_opaque(sys::JS_GetRuntime(ctx), this_val);
    if !ptr.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            (*ptr).borrow_mut().set_font(&s);
        }
    }
    sys::js_undefined()
}
