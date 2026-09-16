//! `FocusEvent`/`SubmitEvent`/`DragEvent`/`TouchEvent` constructors — split
//! out from `event_subclasses/mod.rs`.
use quickjs_sys as sys;
use std::os::raw::c_int;

use super::{
    constructor_prototype, options_arg, read_bool_option, read_number_option, read_type_arg,
    read_value_option, set_bool_prop, set_number_prop, set_value_prop,
};

pub(super) unsafe extern "C" fn focus_event_constructor(
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

pub(super) unsafe extern "C" fn submit_event_constructor(
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

pub(super) unsafe extern "C" fn drag_event_constructor(
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

pub(super) unsafe extern "C" fn touch_event_constructor(
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
