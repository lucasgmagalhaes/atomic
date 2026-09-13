//! Real per-element `scrollTop`/`scrollLeft`/`scroll()`/`scrollTo()`/
//! `scrollBy()` (`ROADMAP.md` item 22), plus real `focus()`/`blur()`, all
//! defined on `Element.prototype`/`Node.prototype` respectively.
//!
//! `scrollTop`/`scrollLeft` read/write `dom::Dom::element_scroll_offset`
//! (see that module's own doc), clamped to `[0, scrollHeight -
//! clientHeight]`/`[0, scrollWidth - clientWidth]` using the same real
//! `layout_measurement` snapshot `getBoundingClientRect`/`scrollHeight`
//! already read — a container whose content doesn't overflow it clamps to
//! `[0, 0]`, matching a real non-scrollable element rejecting any nonzero
//! `scrollTop`. `scroll()`/`scrollTo()` set an absolute position;
//! `scrollBy()` adds a delta to the current one before the same clamp —
//! same `(x, y)` vs `{top, left}` argument shapes real CSSOM defines
//! (`behavior` is accepted and ignored, no smooth-scroll animation
//! exists, matching `window.scrollTo`'s own precedent). `scrollIntoView()`
//! stays a no-op - out of this item's scope.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_opaque};
use super::util::{Getter, Setter};

pub(super) unsafe fn define_scroll_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, getter, setter) in [
        (
            "scrollTop",
            scroll_top_get as Getter,
            scroll_top_set as Setter,
        ),
        (
            "scrollLeft",
            scroll_left_get as Getter,
            scroll_left_set as Setter,
        ),
    ] {
        let cname = CString::new(name).unwrap();
        let getter_fn = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<Getter, sys::JSCFunction>(getter),
            cname.as_ptr(),
            0,
            sys::JS_CFUNC_GETTER,
            0,
        );
        let setter_fn = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<Setter, sys::JSCFunction>(setter),
            cname.as_ptr(),
            1,
            sys::JS_CFUNC_SETTER,
            0,
        );
        let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
        sys::JS_DefinePropertyGetSet(
            ctx,
            proto,
            atom,
            getter_fn,
            setter_fn,
            sys::JS_PROP_HAS_GET
                | sys::JS_PROP_HAS_SET
                | sys::JS_PROP_CONFIGURABLE
                | sys::JS_PROP_ENUMERABLE,
        );
        sys::JS_FreeAtom(ctx, atom);
    }

    for (name, func) in [
        ("scroll", element_scroll_to as sys::JSCFunction),
        ("scrollTo", element_scroll_to as sys::JSCFunction),
        ("scrollBy", element_scroll_by as sys::JSCFunction),
        (
            "scrollIntoView",
            element_scroll_into_view_noop as sys::JSCFunction,
        ),
    ] {
        let cname = CString::new(name).unwrap();
        let value = sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), 0, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, proto, cname.as_ptr(), value);
    }
}

unsafe fn current_offset(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> (f64, f64) {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    if node_ptr.is_null() {
        return (0.0, 0.0);
    }
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return (0.0, 0.0);
    }
    (*dom_ptr).element_scroll_offset(*node_ptr)
}

/// Clamps a requested `(x, y)` scroll offset to `[0, scrollWidth -
/// clientWidth]`/`[0, scrollHeight - clientHeight]` and writes it via
/// `dom::Dom::set_element_scroll_offset`. No-op if `this_val` has no
/// backing node/DOM.
unsafe fn write_clamped_offset(ctx: *mut sys::JSContext, this_val: sys::JSValue, x: f64, y: f64) {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    if node_ptr.is_null() {
        return;
    }
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return;
    }
    let client = crate::layout_measurement::rect_for(ctx, this_val);
    let (scroll_width, scroll_height) = crate::layout_measurement::scroll_extent_for(ctx, this_val);
    let max_x = (scroll_width - client.width).max(0.0);
    let max_y = (scroll_height - client.height).max(0.0);
    let clamped_x = x.max(0.0).min(max_x);
    let clamped_y = y.max(0.0).min(max_y);
    (*dom_ptr).set_element_scroll_offset(*node_ptr, clamped_x, clamped_y);
}

unsafe extern "C" fn scroll_top_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(current_offset(ctx, this_val).1)
}

unsafe extern "C" fn scroll_left_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(current_offset(ctx, this_val).0)
}

unsafe extern "C" fn scroll_top_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    if let Some(y) = read_number(val) {
        let x = current_offset(ctx, this_val).0;
        write_clamped_offset(ctx, this_val, x, y);
    }
    sys::js_undefined()
}

unsafe extern "C" fn scroll_left_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    if let Some(x) = read_number(val) {
        let y = current_offset(ctx, this_val).1;
        write_clamped_offset(ctx, this_val, x, y);
    }
    sys::js_undefined()
}

unsafe fn read_number(val: sys::JSValue) -> Option<f64> {
    match val.tag {
        sys::JS_TAG_INT => Some(val.u.int32 as f64),
        sys::JS_TAG_FLOAT64 => Some(val.u.float64),
        _ => None,
    }
}

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str) -> sys::JSValue {
    let name = CString::new(key).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

/// Reads `scroll`/`scrollTo`/`scrollBy`'s real per-spec argument shape:
/// either two numeric coordinates (`x`, `y`) or a single
/// `ScrollToOptions`-shaped object (`{top, left, behavior}` - `behavior`
/// accepted and ignored). `None` for a coordinate not supplied at all -
/// same convention `window.rs`'s own `read_scroll_args` already uses.
unsafe fn read_scroll_args(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> (Option<f64>, Option<f64>) {
    if argc < 1 {
        return (None, None);
    }
    let first = *argv;
    if first.tag == sys::JS_TAG_OBJECT {
        let top = get_prop(ctx, first, "top");
        let y = read_number(top);
        sys::JS_FreeValue(ctx, top);
        let left = get_prop(ctx, first, "left");
        let x = read_number(left);
        sys::JS_FreeValue(ctx, left);
        return (x, y);
    }
    let x = read_number(first);
    let y = if argc >= 2 {
        read_number(*argv.add(1))
    } else {
        None
    };
    (x, y)
}

unsafe extern "C" fn element_scroll_to(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let (x, y) = read_scroll_args(ctx, argc, argv);
    let current = current_offset(ctx, this_val);
    write_clamped_offset(
        ctx,
        this_val,
        x.unwrap_or(current.0),
        y.unwrap_or(current.1),
    );
    sys::js_undefined()
}

unsafe extern "C" fn element_scroll_by(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let (dx, dy) = read_scroll_args(ctx, argc, argv);
    let current = current_offset(ctx, this_val);
    write_clamped_offset(
        ctx,
        this_val,
        current.0 + dx.unwrap_or(0.0),
        current.1 + dy.unwrap_or(0.0),
    );
    sys::js_undefined()
}

/// Also used by `forms.rs`'s `define_form_properties` for the no-op
/// `submit()`/`reset()` methods.
pub(super) unsafe extern "C" fn element_scroll_noop(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    sys::js_undefined()
}

unsafe extern "C" fn element_scroll_into_view_noop(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    sys::js_undefined()
}

unsafe extern "C" fn node_focus(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        (*dom_ptr).focus(*node_ptr);
        crate::events::dispatch(ctx, this_val, "focus");
    }
    sys::js_undefined()
}

unsafe extern "C" fn node_blur(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        // `Some(changed)` only when `id` really was the focused node —
        // matches the real DOM not firing `"blur"` for an element that
        // wasn't focused to begin with.
        if let Some(changed) = (*dom_ptr).blur(*node_ptr) {
            crate::events::dispatch(ctx, this_val, "blur");
            if changed {
                crate::events::dispatch(ctx, this_val, "change");
            }
        }
    }
    sys::js_undefined()
}

/// Defines real `focus()`/`blur()` methods on `proto`, backed by
/// `dom::Dom`'s own focus state (`document.activeElement`, see
/// `document::document_active_element_get`).
pub(super) unsafe fn define_focus_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let focus_name = CString::new("focus").unwrap();
    let focus_fn = sys::JS_NewCFunction2(
        ctx,
        node_focus,
        focus_name.as_ptr(),
        0,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, proto, focus_name.as_ptr(), focus_fn);

    let blur_name = CString::new("blur").unwrap();
    let blur_fn = sys::JS_NewCFunction2(
        ctx,
        node_blur,
        blur_name.as_ptr(),
        0,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, proto, blur_name.as_ptr(), blur_fn);
}
