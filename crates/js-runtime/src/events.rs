//! `addEventListener`/`removeEventListener`/`dispatchEvent` on `Node`.
//! Deliberate cuts from the real EventTarget spec:
//! - one listener per (node, event type), last registered wins — no
//!   listener list, no capture/bubble phases, no `once`/`passive` options.
//! - `dispatchEvent(type)` takes a plain string, not a real `Event` object
//!   (no `Event`/`CustomEvent` class exists yet) — the listener is called
//!   with no arguments, so it can't read `event.type` or anything else.
//! Storage: an own `__listeners` object property on each node instance
//! (created lazily), keyed by event type name. Plain JS objects/properties
//! only — no new native state, reusing bindings already added for
//! `document`/DOM.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

const LISTENERS_PROP: &[u8] = b"__listeners\0";

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
    let mut len: usize = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, val, false);
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
    let s = String::from_utf8_lossy(bytes).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

/// Gets (creating if absent) the `__listeners` object own to `this_val`.
/// Returns an owned reference.
unsafe fn get_or_create_listeners(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue {
    let prop = LISTENERS_PROP.as_ptr() as *const std::os::raw::c_char;
    let existing = sys::JS_GetPropertyStr(ctx, this_val, prop);
    if existing.tag != sys::JS_TAG_UNDEFINED {
        return existing;
    }
    sys::JS_FreeValue(ctx, existing);
    let listeners = sys::JS_NewObject(ctx);
    sys::JS_SetPropertyStr(ctx, this_val, prop, sys::JS_DupValue(ctx, listeners));
    listeners
}

unsafe extern "C" fn add_event_listener(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return sys::js_undefined();
    }
    let Some(event_type) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let callback = *argv.add(1);

    let listeners = get_or_create_listeners(ctx, this_val);
    let type_name = CString::new(event_type).unwrap_or_default();
    sys::JS_SetPropertyStr(ctx, listeners, type_name.as_ptr(), sys::JS_DupValue(ctx, callback));
    sys::JS_FreeValue(ctx, listeners);

    sys::js_undefined()
}

unsafe extern "C" fn remove_event_listener(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let Some(event_type) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };

    let listeners = get_or_create_listeners(ctx, this_val);
    let type_name = CString::new(event_type).unwrap_or_default();
    // No JS_DeleteProperty binding - overwriting with undefined is
    // equivalent for dispatch_event's purposes (it checks the tag).
    sys::JS_SetPropertyStr(ctx, listeners, type_name.as_ptr(), sys::js_undefined());
    sys::JS_FreeValue(ctx, listeners);

    sys::js_undefined()
}

unsafe extern "C" fn dispatch_event(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_bool(false);
    }
    let Some(event_type) = read_js_string(ctx, *argv) else {
        return sys::js_bool(false);
    };

    let listeners = get_or_create_listeners(ctx, this_val);
    let type_name = CString::new(event_type).unwrap_or_default();
    let listener = sys::JS_GetPropertyStr(ctx, listeners, type_name.as_ptr());
    sys::JS_FreeValue(ctx, listeners);

    if listener.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, listener);
        return sys::js_bool(false);
    }

    let result = sys::JS_Call(ctx, listener, this_val, 0, std::ptr::null_mut());
    sys::JS_FreeValue(ctx, result);
    sys::JS_FreeValue(ctx, listener);
    sys::js_bool(true)
}

/// Adds `addEventListener`/`removeEventListener`/`dispatchEvent` to
/// `proto`. Called from `dom_bindings::ensure_node_class` alongside
/// `define_text_content`, before the prototype is handed to
/// `JS_SetClassProto`.
pub(crate) unsafe fn define_event_target(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let methods: &[(&str, sys::JSCFunction, c_int)] = &[
        ("addEventListener", add_event_listener, 2),
        ("removeEventListener", remove_event_listener, 1),
        ("dispatchEvent", dispatch_event, 1),
    ];
    for (name, func, arity) in methods {
        let cname = CString::new(*name).unwrap();
        let value = sys::JS_NewCFunction2(ctx, *func, cname.as_ptr(), *arity, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, proto, cname.as_ptr(), value);
    }
}
