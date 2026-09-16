//! `PointerEvent`/`MouseEvent`/`WheelEvent` constructors — split out from
//! `event_subclasses/mod.rs`.
use quickjs_sys as sys;
use std::os::raw::c_int;

use super::{
    constructor_prototype, options_arg, read_bool_option, read_number_option, read_string_option,
    read_type_arg, read_value_option, set_bool_prop, set_number_prop, set_string_prop,
    set_value_prop,
};

pub(super) unsafe extern "C" fn pointer_event_constructor(
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

pub(super) unsafe extern "C" fn mouse_event_constructor(
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

pub(super) unsafe extern "C" fn wheel_event_constructor(
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
