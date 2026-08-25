//! `CustomEvent`/`KeyboardEvent`/`PointerEvent`/`MouseEvent`/`FocusEvent`/`InputEvent`/`WheelEvent`/`SubmitEvent`/`DragEvent`/`TouchEvent` —
//! thin globals built on top of `events::create_event`. None of these know
//! how `EventState` is represented; the subclass-specific fields (`detail`,
//! `key`, `pointerId`, `clientX`, `relatedTarget`, ...) are plain own data
//! properties on top of the base `Event`-class object, not extra native
//! state, so they ride through the existing `dispatchEvent`/`EventState`
//! machinery in `events.rs` unmodified.
use quickjs_sys as sys;
use std::ffi::CString;
use std::os::raw::c_int;

unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}
unsafe fn throw_type_error(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_string(ctx, s))
}
unsafe fn read_string(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<String> {
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

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &str) -> sys::JSValue {
    let name = CString::new(name).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

/// Reads `options[name]` as a bool, defaulting to `default` when the
/// property is absent/`undefined`. Mirrors `events.rs::event_constructor`'s
/// own `bubbles`/`cancelable` handling.
unsafe fn read_bool_option(
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

unsafe fn read_string_option(
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

unsafe fn read_number_option(
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
unsafe fn read_value_option(
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

unsafe fn set_string_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &str, value: &str) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), new_string(ctx, value));
}
unsafe fn set_number_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &str, value: f64) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_float64(value));
}
unsafe fn set_bool_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &str, value: bool) {
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), sys::js_bool(value));
}
/// Consumes `value` (an owned reference) — matches `JS_SetPropertyStr`'s own
/// "takes ownership of the value" contract, same as every other `set_*_prop`
/// helper here just with no primitive conversion in the way.
unsafe fn set_value_prop(
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
unsafe fn constructor_prototype(ctx: *mut sys::JSContext, name: &str) -> sys::JSValue {
    let global = sys::JS_GetGlobalObject(ctx);
    let cname = CString::new(name).unwrap();
    let ctor = sys::JS_GetPropertyStr(ctx, global, cname.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let proto_name = CString::new("prototype").unwrap();
    let proto = sys::JS_GetPropertyStr(ctx, ctor, proto_name.as_ptr());
    sys::JS_FreeValue(ctx, ctor);
    proto
}

unsafe fn read_type_arg(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> Result<String, sys::JSValue> {
    if argc < 1 {
        return Err(throw_type_error(ctx, "event type is required"));
    }
    read_string(ctx, *argv).ok_or_else(|| throw_type_error(ctx, "event type must be a string"))
}

unsafe fn options_arg(argc: c_int, argv: *mut sys::JSValue) -> sys::JSValue {
    if argc >= 2 {
        *argv.add(1)
    } else {
        sys::js_undefined()
    }
}

unsafe extern "C" fn custom_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    let detail = if options.tag == sys::JS_TAG_UNDEFINED {
        sys::js_null()
    } else {
        let value = get_prop(ctx, options, "detail");
        if sys::js_is_exception(&value) {
            return value;
        }
        if value.tag == sys::JS_TAG_UNDEFINED {
            sys::JS_FreeValue(ctx, value);
            sys::js_null()
        } else {
            value
        }
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        sys::JS_FreeValue(ctx, detail);
        return event;
    }
    let detail_name = CString::new("detail").unwrap();
    sys::JS_SetPropertyStr(ctx, event, detail_name.as_ptr(), detail);
    let proto = constructor_prototype(ctx, "CustomEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}

unsafe extern "C" fn keyboard_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    let Some(key) = read_string_option(ctx, options, "key", "") else {
        return sys::js_exception();
    };
    let Some(code) = read_string_option(ctx, options, "code", "") else {
        return sys::js_exception();
    };
    let Some(key_code) = read_number_option(ctx, options, "keyCode", 0.0) else {
        return sys::js_exception();
    };
    let Some(ctrl_key) = read_bool_option(ctx, options, "ctrlKey", false) else {
        return sys::js_exception();
    };
    let Some(shift_key) = read_bool_option(ctx, options, "shiftKey", false) else {
        return sys::js_exception();
    };
    let Some(alt_key) = read_bool_option(ctx, options, "altKey", false) else {
        return sys::js_exception();
    };
    let Some(meta_key) = read_bool_option(ctx, options, "metaKey", false) else {
        return sys::js_exception();
    };
    let Some(repeat) = read_bool_option(ctx, options, "repeat", false) else {
        return sys::js_exception();
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        return event;
    }
    set_string_prop(ctx, event, "key", &key);
    set_string_prop(ctx, event, "code", &code);
    set_number_prop(ctx, event, "keyCode", key_code);
    set_bool_prop(ctx, event, "ctrlKey", ctrl_key);
    set_bool_prop(ctx, event, "shiftKey", shift_key);
    set_bool_prop(ctx, event, "altKey", alt_key);
    set_bool_prop(ctx, event, "metaKey", meta_key);
    set_bool_prop(ctx, event, "repeat", repeat);
    let proto = constructor_prototype(ctx, "KeyboardEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}

unsafe extern "C" fn pointer_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    let Some(pointer_id) = read_number_option(ctx, options, "pointerId", 0.0) else {
        return sys::js_exception();
    };
    let Some(pointer_type) = read_string_option(ctx, options, "pointerType", "") else {
        return sys::js_exception();
    };
    let Some(client_x) = read_number_option(ctx, options, "clientX", 0.0) else {
        return sys::js_exception();
    };
    let Some(client_y) = read_number_option(ctx, options, "clientY", 0.0) else {
        return sys::js_exception();
    };
    let Some(button) = read_number_option(ctx, options, "button", 0.0) else {
        return sys::js_exception();
    };
    let Some(buttons) = read_number_option(ctx, options, "buttons", 0.0) else {
        return sys::js_exception();
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        return event;
    }
    set_number_prop(ctx, event, "pointerId", pointer_id);
    set_string_prop(ctx, event, "pointerType", &pointer_type);
    set_number_prop(ctx, event, "clientX", client_x);
    set_number_prop(ctx, event, "clientY", client_y);
    set_number_prop(ctx, event, "button", button);
    set_number_prop(ctx, event, "buttons", buttons);
    let proto = constructor_prototype(ctx, "PointerEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}

unsafe extern "C" fn mouse_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    let Some(screen_x) = read_number_option(ctx, options, "screenX", 0.0) else {
        return sys::js_exception();
    };
    let Some(screen_y) = read_number_option(ctx, options, "screenY", 0.0) else {
        return sys::js_exception();
    };
    let Some(client_x) = read_number_option(ctx, options, "clientX", 0.0) else {
        return sys::js_exception();
    };
    let Some(client_y) = read_number_option(ctx, options, "clientY", 0.0) else {
        return sys::js_exception();
    };
    let Some(ctrl_key) = read_bool_option(ctx, options, "ctrlKey", false) else {
        return sys::js_exception();
    };
    let Some(shift_key) = read_bool_option(ctx, options, "shiftKey", false) else {
        return sys::js_exception();
    };
    let Some(alt_key) = read_bool_option(ctx, options, "altKey", false) else {
        return sys::js_exception();
    };
    let Some(meta_key) = read_bool_option(ctx, options, "metaKey", false) else {
        return sys::js_exception();
    };
    let Some(button) = read_number_option(ctx, options, "button", 0.0) else {
        return sys::js_exception();
    };
    let Some(buttons) = read_number_option(ctx, options, "buttons", 0.0) else {
        return sys::js_exception();
    };
    let Some(related_target) = read_value_option(ctx, options, "relatedTarget") else {
        return sys::js_exception();
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        sys::JS_FreeValue(ctx, related_target);
        return event;
    }
    set_number_prop(ctx, event, "screenX", screen_x);
    set_number_prop(ctx, event, "screenY", screen_y);
    set_number_prop(ctx, event, "clientX", client_x);
    set_number_prop(ctx, event, "clientY", client_y);
    set_bool_prop(ctx, event, "ctrlKey", ctrl_key);
    set_bool_prop(ctx, event, "shiftKey", shift_key);
    set_bool_prop(ctx, event, "altKey", alt_key);
    set_bool_prop(ctx, event, "metaKey", meta_key);
    set_number_prop(ctx, event, "button", button);
    set_number_prop(ctx, event, "buttons", buttons);
    set_value_prop(ctx, event, "relatedTarget", related_target);
    let proto = constructor_prototype(ctx, "MouseEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}

unsafe extern "C" fn focus_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    let Some(related_target) = read_value_option(ctx, options, "relatedTarget") else {
        return sys::js_exception();
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        sys::JS_FreeValue(ctx, related_target);
        return event;
    }
    set_value_prop(ctx, event, "relatedTarget", related_target);
    let proto = constructor_prototype(ctx, "FocusEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}

unsafe extern "C" fn input_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    let Some(data) = read_value_option(ctx, options, "data") else {
        return sys::js_exception();
    };
    let Some(input_type) = read_string_option(ctx, options, "inputType", "") else {
        sys::JS_FreeValue(ctx, data);
        return sys::js_exception();
    };
    let Some(is_composing) = read_bool_option(ctx, options, "isComposing", false) else {
        sys::JS_FreeValue(ctx, data);
        return sys::js_exception();
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        sys::JS_FreeValue(ctx, data);
        return event;
    }
    set_value_prop(ctx, event, "data", data);
    set_string_prop(ctx, event, "inputType", &input_type);
    set_bool_prop(ctx, event, "isComposing", is_composing);
    let proto = constructor_prototype(ctx, "InputEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}

unsafe extern "C" fn wheel_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    let Some(delta_x) = read_number_option(ctx, options, "deltaX", 0.0) else {
        return sys::js_exception();
    };
    let Some(delta_y) = read_number_option(ctx, options, "deltaY", 0.0) else {
        return sys::js_exception();
    };
    let Some(delta_z) = read_number_option(ctx, options, "deltaZ", 0.0) else {
        return sys::js_exception();
    };
    // 0 = DOM_DELTA_PIXEL, the real spec's own default.
    let Some(delta_mode) = read_number_option(ctx, options, "deltaMode", 0.0) else {
        return sys::js_exception();
    };
    let Some(client_x) = read_number_option(ctx, options, "clientX", 0.0) else {
        return sys::js_exception();
    };
    let Some(client_y) = read_number_option(ctx, options, "clientY", 0.0) else {
        return sys::js_exception();
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        return event;
    }
    set_number_prop(ctx, event, "deltaX", delta_x);
    set_number_prop(ctx, event, "deltaY", delta_y);
    set_number_prop(ctx, event, "deltaZ", delta_z);
    set_number_prop(ctx, event, "deltaMode", delta_mode);
    set_number_prop(ctx, event, "clientX", client_x);
    set_number_prop(ctx, event, "clientY", client_y);
    let proto = constructor_prototype(ctx, "WheelEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}

unsafe extern "C" fn submit_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    let Some(submitter) = read_value_option(ctx, options, "submitter") else {
        return sys::js_exception();
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        sys::JS_FreeValue(ctx, submitter);
        return event;
    }
    set_value_prop(ctx, event, "submitter", submitter);
    let proto = constructor_prototype(ctx, "SubmitEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}

unsafe extern "C" fn drag_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    // MouseEvent-inherited fields
    let Some(screen_x) = read_number_option(ctx, options, "screenX", 0.0) else {
        return sys::js_exception();
    };
    let Some(screen_y) = read_number_option(ctx, options, "screenY", 0.0) else {
        return sys::js_exception();
    };
    let Some(client_x) = read_number_option(ctx, options, "clientX", 0.0) else {
        return sys::js_exception();
    };
    let Some(client_y) = read_number_option(ctx, options, "clientY", 0.0) else {
        return sys::js_exception();
    };
    let Some(ctrl_key) = read_bool_option(ctx, options, "ctrlKey", false) else {
        return sys::js_exception();
    };
    let Some(shift_key) = read_bool_option(ctx, options, "shiftKey", false) else {
        return sys::js_exception();
    };
    let Some(alt_key) = read_bool_option(ctx, options, "altKey", false) else {
        return sys::js_exception();
    };
    let Some(meta_key) = read_bool_option(ctx, options, "metaKey", false) else {
        return sys::js_exception();
    };
    let Some(button) = read_number_option(ctx, options, "button", 0.0) else {
        return sys::js_exception();
    };
    let Some(buttons) = read_number_option(ctx, options, "buttons", 0.0) else {
        return sys::js_exception();
    };
    let Some(related_target) = read_value_option(ctx, options, "relatedTarget") else {
        return sys::js_exception();
    };
    let Some(data_transfer) = read_value_option(ctx, options, "dataTransfer") else {
        sys::JS_FreeValue(ctx, related_target);
        return sys::js_exception();
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        sys::JS_FreeValue(ctx, related_target);
        sys::JS_FreeValue(ctx, data_transfer);
        return event;
    }
    set_number_prop(ctx, event, "screenX", screen_x);
    set_number_prop(ctx, event, "screenY", screen_y);
    set_number_prop(ctx, event, "clientX", client_x);
    set_number_prop(ctx, event, "clientY", client_y);
    set_bool_prop(ctx, event, "ctrlKey", ctrl_key);
    set_bool_prop(ctx, event, "shiftKey", shift_key);
    set_bool_prop(ctx, event, "altKey", alt_key);
    set_bool_prop(ctx, event, "metaKey", meta_key);
    set_number_prop(ctx, event, "button", button);
    set_number_prop(ctx, event, "buttons", buttons);
    set_value_prop(ctx, event, "relatedTarget", related_target);
    set_value_prop(ctx, event, "dataTransfer", data_transfer);
    let proto = constructor_prototype(ctx, "DragEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}

unsafe extern "C" fn touch_event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let kind = match read_type_arg(ctx, argc, argv) {
        Ok(k) => k,
        Err(e) => return e,
    };
    let options = options_arg(argc, argv);
    let Some(bubbles) = read_bool_option(ctx, options, "bubbles", false) else {
        return sys::js_exception();
    };
    let Some(cancelable) = read_bool_option(ctx, options, "cancelable", false) else {
        return sys::js_exception();
    };
    let Some(touches) = read_value_option(ctx, options, "touches") else {
        return sys::js_exception();
    };
    let Some(target_touches) = read_value_option(ctx, options, "targetTouches") else {
        sys::JS_FreeValue(ctx, touches);
        return sys::js_exception();
    };
    let Some(changed_touches) = read_value_option(ctx, options, "changedTouches") else {
        sys::JS_FreeValue(ctx, touches);
        sys::JS_FreeValue(ctx, target_touches);
        return sys::js_exception();
    };
    let Some(alt_key) = read_bool_option(ctx, options, "altKey", false) else {
        sys::JS_FreeValue(ctx, touches);
        sys::JS_FreeValue(ctx, target_touches);
        sys::JS_FreeValue(ctx, changed_touches);
        return sys::js_exception();
    };
    let Some(meta_key) = read_bool_option(ctx, options, "metaKey", false) else {
        sys::JS_FreeValue(ctx, touches);
        sys::JS_FreeValue(ctx, target_touches);
        sys::JS_FreeValue(ctx, changed_touches);
        return sys::js_exception();
    };
    let Some(ctrl_key) = read_bool_option(ctx, options, "ctrlKey", false) else {
        sys::JS_FreeValue(ctx, touches);
        sys::JS_FreeValue(ctx, target_touches);
        sys::JS_FreeValue(ctx, changed_touches);
        return sys::js_exception();
    };
    let Some(shift_key) = read_bool_option(ctx, options, "shiftKey", false) else {
        sys::JS_FreeValue(ctx, touches);
        sys::JS_FreeValue(ctx, target_touches);
        sys::JS_FreeValue(ctx, changed_touches);
        return sys::js_exception();
    };

    let event = crate::events::create_event(ctx, &kind, bubbles, cancelable);
    if sys::js_is_exception(&event) {
        sys::JS_FreeValue(ctx, touches);
        sys::JS_FreeValue(ctx, target_touches);
        sys::JS_FreeValue(ctx, changed_touches);
        return event;
    }
    set_value_prop(ctx, event, "touches", touches);
    set_value_prop(ctx, event, "targetTouches", target_touches);
    set_value_prop(ctx, event, "changedTouches", changed_touches);
    set_bool_prop(ctx, event, "altKey", alt_key);
    set_bool_prop(ctx, event, "metaKey", meta_key);
    set_bool_prop(ctx, event, "ctrlKey", ctrl_key);
    set_bool_prop(ctx, event, "shiftKey", shift_key);
    let proto = constructor_prototype(ctx, "TouchEvent");
    sys::JS_SetPrototype(ctx, event, proto);
    sys::JS_FreeValue(ctx, proto);
    event
}

/// Registers one `name` global: a plain prototype object chained onto
/// `Event.prototype` (so `instanceof Event` and inherited accessors like
/// `preventDefault`/`type` keep working — see `events.rs::register`'s own
/// `JS_GetOpaque`-on-`this_val` trick, which doesn't care which prototype
/// object sits in between), plus a constructor function whose own
/// `.prototype` points back at it.
unsafe fn register_subclass(
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
