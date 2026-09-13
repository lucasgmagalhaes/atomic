//! `CustomEvent`/`KeyboardEvent`/`PointerEvent`/`MouseEvent`/`FocusEvent`/`InputEvent`/`WheelEvent`/`SubmitEvent`/`DragEvent`/`TouchEvent` —
//! thin globals built on top of `events::create_event`. None of these know
//! how `EventState` is represented; the subclass-specific fields (`detail`,
//! `key`, `pointerId`, `clientX`, `relatedTarget`, ...) are plain own data
//! properties on top of the base `Event`-class object, not extra native
//! state, so they ride through the existing `dispatchEvent`/`EventState`
//! machinery in `events.rs` unmodified.
//!
//! Split into `pointer_mouse_wheel.rs` (`PointerEvent`/`MouseEvent`/
//! `WheelEvent`) and `focus_submit_drag_touch.rs` (`FocusEvent`/
//! `SubmitEvent`/`DragEvent`/`TouchEvent`) — this file keeps the shared
//! read/write helpers, `CustomEvent`/`KeyboardEvent`/`InputEvent`, and the
//! single public entry point, `register`.
use quickjs_sys as sys;
use std::ffi::CString;
use std::os::raw::c_int;

mod custom_keyboard_input;
mod focus_submit_drag_touch;
mod pointer_mouse_wheel;

use custom_keyboard_input::{
    custom_event_constructor, input_event_constructor, keyboard_event_constructor,
};
use focus_submit_drag_touch::{
    drag_event_constructor, focus_event_constructor, submit_event_constructor,
    touch_event_constructor,
};
use pointer_mouse_wheel::{
    mouse_event_constructor, pointer_event_constructor, wheel_event_constructor,
};

pub(super) unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}
unsafe fn throw_type_error(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_string(ctx, s))
}
pub(super) unsafe fn read_string(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<String> {
    let mut len = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, value, false);
    if ptr.is_null() {
        return None;
    }
    let s = String::from_utf8_lossy(std::slice::from_raw_parts(ptr as *const u8, len)).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}
/// Reads a JS number value already tagged `INT`/`FLOAT64` (no string- or
/// object-to-number coercion — no `JS_ToFloat64` binding exists yet, same
/// scope cut `blob.rs`/`value_bridge.rs` already document).
unsafe fn read_number(val: sys::JSValue) -> Option<f64> {
    match val.tag {
        sys::JS_TAG_INT => Some(val.u.int32 as f64),
        sys::JS_TAG_FLOAT64 => Some(val.u.float64),
        _ => None,
    }
}

pub(super) unsafe fn get_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
) -> sys::JSValue {
    let name = CString::new(name).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

/// Reads `options[name]` as a bool, defaulting to `default` when the
/// property is absent/`undefined`. Mirrors `events.rs::event_constructor`'s
/// own `bubbles`/`cancelable` handling.
pub(super) unsafe fn read_bool_option(
    ctx: *mut sys::JSContext,
    options: sys::JSValue,
    name: &str,
    default: bool,
) -> Option<bool> {
    if options.tag == sys::JS_TAG_UNDEFINED {
        return Some(default);
    }
    let value = get_prop(ctx, options, name);
    if sys::js_is_exception(&value) {
        return None;
    }
    if value.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, value);
        return Some(default);
    }
    let b = sys::JS_ToBool(ctx, value);
    sys::JS_FreeValue(ctx, value);
    if b < 0 {
        return None;
    }
    Some(b != 0)
}

pub(super) unsafe fn read_string_option(
    ctx: *mut sys::JSContext,
    options: sys::JSValue,
    name: &str,
    default: &str,
) -> Option<String> {
    if options.tag == sys::JS_TAG_UNDEFINED {
        return Some(default.to_string());
    }
    let value = get_prop(ctx, options, name);
    if sys::js_is_exception(&value) {
        return None;
    }
    if value.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, value);
        return Some(default.to_string());
    }
    let s = read_string(ctx, value);
    sys::JS_FreeValue(ctx, value);
    s
}

pub(super) unsafe fn read_number_option(
    ctx: *mut sys::JSContext,
    options: sys::JSValue,
    name: &str,
    default: f64,
) -> Option<f64> {
    if options.tag == sys::JS_TAG_UNDEFINED {
        return Some(default);
    }
    let value = get_prop(ctx, options, name);
    if sys::js_is_exception(&value) {
        return None;
    }
    if value.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, value);
        return Some(default);
    }
    let n = read_number(value);
    sys::JS_FreeValue(ctx, value);
    n.or(Some(default))
}

/// Reads `options[name]` as a raw `JSValue`, defaulting to `null` when the
/// property is absent/`undefined`. Used for `relatedTarget`, which is a
/// `Node` reference (or `null`), not a primitive — no coercion attempted,
/// matching this file's other `read_*_option` helpers' "no object-to-X
/// coercion" scope cut.
pub(super) unsafe fn read_value_option(
    ctx: *mut sys::JSContext,
    options: sys::JSValue,
    name: &str,
) -> Option<sys::JSValue> {
    if options.tag == sys::JS_TAG_UNDEFINED {
        return Some(sys::js_null());
    }
    let value = get_prop(ctx, options, name);
    if sys::js_is_exception(&value) {
        return None;
    }
    if value.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, value);
        return Some(sys::js_null());
    }
    Some(value)
}

pub(super) unsafe fn set_string_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    value: &str,
) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), new_string(ctx, value));
}
pub(super) unsafe fn set_number_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    value: f64,
) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_float64(value));
}
pub(super) unsafe fn set_bool_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    value: bool,
) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_bool(value));
}
/// Consumes `value` (an owned reference) — matches `JS_SetPropertyStr`'s own
/// "takes ownership of the value" contract, same as every other `set_*_prop`
/// helper here just with no primitive conversion in the way.
pub(super) unsafe fn set_value_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    value: sys::JSValue,
) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), value);
}

/// Fetches `globalThis.<name>.prototype` — same pattern as
/// `dom_bindings::array_prototype`, generalized to any global constructor.
/// Works for `Event` itself too: `events.rs::register` sets both the
/// internal per-context class proto (`JS_SetClassProto`, for
/// `JS_NewObjectClass`) and this same object as `Event`'s own JS-visible
/// `prototype` property, so this is the one real prototype object either
/// path resolves to.
pub(super) unsafe fn constructor_prototype(ctx: *mut sys::JSContext, name: &str) -> sys::JSValue {
    let global = sys::JS_GetGlobalObject(ctx);
    let cname = CString::new(name).unwrap();
    let ctor = sys::JS_GetPropertyStr(ctx, global, cname.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let proto_name = CString::new("prototype").unwrap();
    let proto = sys::JS_GetPropertyStr(ctx, ctor, proto_name.as_ptr());
    sys::JS_FreeValue(ctx, ctor);
    proto
}

pub(super) unsafe fn read_type_arg(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> Result<String, sys::JSValue> {
    if argc < 1 {
        return Err(throw_type_error(ctx, "event type is required"));
    }
    read_string(ctx, *argv).ok_or_else(|| throw_type_error(ctx, "event type must be a string"))
}

pub(super) unsafe fn options_arg(argc: c_int, argv: *mut sys::JSValue) -> sys::JSValue {
    if argc >= 2 {
        *argv.add(1)
    } else {
        sys::js_undefined()
    }
}

/// Registers one `name` global: a plain prototype object chained onto
/// `Event.prototype` (so `instanceof Event` and inherited accessors like
/// `preventDefault`/`type` keep working — see `events.rs::register`'s own
/// `JS_GetOpaque`-on-`this_val` trick, which doesn't care which prototype
/// object sits in between), plus a constructor function whose own
/// `.prototype` points back at it.
pub(super) unsafe fn register_subclass(
    ctx: *mut sys::JSContext,
    name: &str,
    constructor_fn: sys::JSCFunction,
) {
    let event_proto = constructor_prototype(ctx, "Event");
    let proto = sys::JS_NewObject(ctx);
    sys::JS_SetPrototype(ctx, proto, event_proto);
    sys::JS_FreeValue(ctx, event_proto);

    let cname = CString::new(name).unwrap();
    let constructor = sys::JS_NewCFunction2(
        ctx,
        constructor_fn,
        cname.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetConstructorBit(ctx, constructor, true);
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(ctx, constructor, proto_name.as_ptr(), proto);

    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, cname.as_ptr(), constructor);
    sys::JS_FreeValue(ctx, global);
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    register_subclass(ctx, "CustomEvent", custom_event_constructor);
    register_subclass(ctx, "KeyboardEvent", keyboard_event_constructor);
    register_subclass(ctx, "PointerEvent", pointer_event_constructor);
    register_subclass(ctx, "MouseEvent", mouse_event_constructor);
    register_subclass(ctx, "FocusEvent", focus_event_constructor);
    register_subclass(ctx, "InputEvent", input_event_constructor);
    register_subclass(ctx, "WheelEvent", wheel_event_constructor);
    register_subclass(ctx, "SubmitEvent", submit_event_constructor);
    register_subclass(ctx, "DragEvent", drag_event_constructor);
    register_subclass(ctx, "TouchEvent", touch_event_constructor);
}
