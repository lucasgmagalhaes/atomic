//! `getBoundingClientRect`/`offsetWidth`/`offsetHeight`/`offsetTop`/
//! `offsetLeft`/`clientWidth`/`clientHeight` — real layout measurement,
//! backed by [`Rect`]s a host (`profile-worker`, after it runs
//! `layout-engine` against the real page) pushes in via
//! `Context::set_layout_rects`. This crate has no dependency on
//! `layout-engine` itself (`Rect` is this module's own minimal shape, not
//! `layout_engine::Dimensions`) — keeping the layout engine out of
//! `js-runtime`'s dependency graph, matching how this crate already keeps
//! `render`/`ipc` out of it too.
//!
//! A node with no entry (never laid out yet, `display: none`, or a host
//! that never calls `set_layout_rects` at all — a plain `Context::with_dom`
//! in a test, say) reads an all-zero rect, matching a real detached/
//! unrendered element's `getBoundingClientRect()`.
//!
//! Deviations from spec, both because this engine has no box-model layers
//! (border/padding/content as distinct boxes) or `offsetParent` chain:
//! `offsetTop`/`offsetLeft` return the same absolute (viewport-relative)
//! coordinates `getBoundingClientRect` does, not a position relative to the
//! nearest positioned ancestor; `clientWidth`/`clientHeight` return the
//! same value as `offsetWidth`/`offsetHeight` (no scrollbar/border/padding
//! subtraction, since `Rect` doesn't carry those separately).
use quickjs_sys as sys;
use std::ffi::CString;

#[derive(Debug, Clone, Copy, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

unsafe fn rect_for(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> Rect {
    let Some(id) = crate::dom_bindings::node_id(ctx, this_val) else {
        return Rect::default();
    };
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return Rect::default();
    }
    (*state).layout_rects.get(&id).copied().unwrap_or_default()
}

unsafe fn new_object(ctx: *mut sys::JSContext) -> sys::JSValue {
    sys::JS_NewObject(ctx)
}
unsafe fn set_number(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &str, value: f64) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_float64(value));
}

unsafe extern "C" fn get_bounding_client_rect(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rect = rect_for(ctx, this_val);
    let object = new_object(ctx);
    set_number(ctx, object, "x", rect.x);
    set_number(ctx, object, "y", rect.y);
    set_number(ctx, object, "width", rect.width);
    set_number(ctx, object, "height", rect.height);
    set_number(ctx, object, "top", rect.y);
    set_number(ctx, object, "left", rect.x);
    set_number(ctx, object, "right", rect.x + rect.width);
    set_number(ctx, object, "bottom", rect.y + rect.height);
    object
}

type Getter = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue) -> sys::JSValue;

unsafe extern "C" fn offset_width_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(rect_for(ctx, this_val).width)
}
unsafe extern "C" fn offset_height_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(rect_for(ctx, this_val).height)
}
unsafe extern "C" fn offset_top_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(rect_for(ctx, this_val).y)
}
unsafe extern "C" fn offset_left_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(rect_for(ctx, this_val).x)
}
unsafe extern "C" fn client_width_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(rect_for(ctx, this_val).width)
}
unsafe extern "C" fn client_height_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_float64(rect_for(ctx, this_val).height)
}

unsafe fn define_readonly(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
    name: &str,
    getter: Getter,
) {
    let cname = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(getter),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        f,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}

pub(crate) unsafe fn define_layout_measurement(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("getBoundingClientRect").unwrap();
    let f = sys::JS_NewCFunction2(
        ctx,
        get_bounding_client_rect,
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, proto, name.as_ptr(), f);

    for (name, getter) in [
        ("offsetWidth", offset_width_get as Getter),
        ("offsetHeight", offset_height_get as Getter),
        ("offsetTop", offset_top_get as Getter),
        ("offsetLeft", offset_left_get as Getter),
        ("clientWidth", client_width_get as Getter),
        ("clientHeight", client_height_get as Getter),
    ] {
        define_readonly(ctx, proto, name, getter);
    }
}
