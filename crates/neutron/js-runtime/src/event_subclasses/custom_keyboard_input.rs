//! `CustomEvent`/`KeyboardEvent`/`InputEvent` constructors — split out from
//! `event_subclasses/mod.rs`.
use quickjs_sys as sys;
use std::ffi::CString;
use std::os::raw::c_int;

use super::{
    constructor_prototype, get_prop, options_arg, read_bool_option, read_number_option,
    read_string_option, read_type_arg, read_value_option, set_bool_prop, set_number_prop,
    set_string_prop, set_value_prop,
};

pub(super) unsafe extern "C" fn custom_event_constructor(
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

pub(super) unsafe extern "C" fn keyboard_event_constructor(
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

pub(super) unsafe extern "C" fn input_event_constructor(
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
