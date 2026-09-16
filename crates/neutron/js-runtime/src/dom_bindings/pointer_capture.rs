//! Real `Element.setPointerCapture`/`releasePointerCapture`/
//! `hasPointerCapture`, backed by `dom::Dom`'s own pointer-capture state
//! (`dom::Dom::set_pointer_capture`/`release_pointer_capture`/
//! `has_pointer_capture`). Defined on `Node.prototype`, same convention
//! `define_focus_methods` already uses for `focus()`/`blur()`.
//!
//! Scope cut (`spec/matrix/events.md` line 17): this engine dispatches no
//! real host-driven `PointerEvent`s at all yet (no `pointerdown`/
//! `pointerup`/`pointermove` — `MOUSE_MOVE`'s own real dispatch is
//! `mouseover`/`mouseout`/`mouseenter`/`mouseleave`, not pointer events),
//! so capture has nothing to redirect *to*. What's real here is the
//! bookkeeping API itself and its `"gotpointercapture"`/
//! `"lostpointercapture"` events - genuinely useful to a page that calls
//! `setPointerCapture` from a real `mousedown`-equivalent handler and
//! later checks `hasPointerCapture`, even without this engine routing
//! synthetic pointer events through the captured target.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_id};

/// Reads a JS number value already tagged `INT`/`FLOAT64` as an `i32`
/// pointer id - no string/object-to-number coercion (no `JS_ToFloat64`
/// binding exists yet, same scope cut `content/selection.rs`'s own
/// `read_number_value` already documents).
unsafe fn read_pointer_id(val: sys::JSValue) -> Option<i32> {
    match val.tag {
        sys::JS_TAG_INT => Some(val.u.int32),
        sys::JS_TAG_FLOAT64 => Some(val.u.float64 as i32),
        _ => None,
    }
}

unsafe extern "C" fn set_pointer_capture(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let Some(pointer_id) = read_pointer_id(*argv) else {
        return sys::js_undefined();
    };
    let dom_ptr = dom_opaque(ctx);
    if !dom_ptr.is_null() {
        if let Some(id) = node_id(ctx, this_val) {
            (*dom_ptr).set_pointer_capture(pointer_id, id);
            crate::events::dispatch(ctx, this_val, "gotpointercapture");
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn release_pointer_capture(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let Some(pointer_id) = read_pointer_id(*argv) else {
        return sys::js_undefined();
    };
    let dom_ptr = dom_opaque(ctx);
    if !dom_ptr.is_null() {
        if let Some(released_from) = (*dom_ptr).release_pointer_capture(pointer_id) {
            if node_id(ctx, this_val) == Some(released_from) {
                crate::events::dispatch(ctx, this_val, "lostpointercapture");
            }
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn has_pointer_capture(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_bool(false);
    }
    let Some(pointer_id) = read_pointer_id(*argv) else {
        return sys::js_bool(false);
    };
    let dom_ptr = dom_opaque(ctx);
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    if dom_ptr.is_null() {
        return sys::js_bool(false);
    }
    sys::js_bool((*dom_ptr).has_pointer_capture(pointer_id, id))
}

pub(super) unsafe fn define_pointer_capture_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, func, argc) in [
        (
            "setPointerCapture",
            set_pointer_capture as sys::JSCFunction,
            1,
        ),
        (
            "releasePointerCapture",
            release_pointer_capture as sys::JSCFunction,
            1,
        ),
        (
            "hasPointerCapture",
            has_pointer_capture as sys::JSCFunction,
            1,
        ),
    ] {
        let cname = CString::new(name).unwrap();
        let f = sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), argc, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, proto, cname.as_ptr(), f);
    }
}
