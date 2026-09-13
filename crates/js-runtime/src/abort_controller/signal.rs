//! `AbortSignal` class, `signal_aborted`, `track_listener`, and
//! `fire_abort` — split out from `abort_controller.rs`.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::events;

use super::helpers::{
    get_prop, new_js_string, read_js_string, resolve_prototype, set_prop, throw_type_error,
    AND_ABORTED, AND_LISTENERS, AND_REASON,
};

/// `true` if `signal` (any `JSValue`, not just a real `AbortSignal`) has
/// already been aborted — `addEventListener` calls this for its `signal`
/// option before ever adding the listener.
pub(crate) unsafe fn signal_aborted(ctx: *mut sys::JSContext, signal: sys::JSValue) -> bool {
    let value = get_prop(ctx, signal, AND_ABORTED);
    let result = sys::JS_ToBool(ctx, value) != 0;
    sys::JS_FreeValue(ctx, value);
    result
}

/// Records that `callback` (registered on `node` for `event_type` with
/// `capture`) should be removed the moment `signal` aborts — see this
/// module's own doc for why this is a plain tracked record rather than a
/// native closure. Dupes `node`/`callback` (the caller keeps its own
/// references); `signal` is borrowed, not consumed.
pub(crate) unsafe fn track_listener(
    ctx: *mut sys::JSContext,
    signal: sys::JSValue,
    node: sys::JSValue,
    event_type: &str,
    callback: sys::JSValue,
    capture: bool,
) {
    let existing = get_prop(ctx, signal, AND_LISTENERS);
    let list = if sys::JS_IsArray(existing) {
        existing
    } else {
        sys::JS_FreeValue(ctx, existing);
        let a = sys::JS_NewArray(ctx);
        set_prop(ctx, signal, AND_LISTENERS, sys::JS_DupValue(ctx, a));
        a
    };
    let record = sys::JS_NewObject(ctx);
    set_prop(ctx, record, b"node\0", sys::JS_DupValue(ctx, node));
    set_prop(ctx, record, b"callback\0", sys::JS_DupValue(ctx, callback));
    set_prop(ctx, record, b"type\0", new_js_string(ctx, event_type));
    set_prop(ctx, record, b"capture\0", sys::js_bool(capture));
    let mut len = 0;
    sys::JS_GetLength(ctx, list, &mut len);
    sys::JS_SetPropertyUint32(ctx, list, len as u32, record);
    sys::JS_FreeValue(ctx, list);
}

/// Marks `signal` aborted with `reason` (consumed), removes every listener
/// [`track_listener`] recorded for it, and fires a real `"abort"` event on
/// it. A no-op (frees `reason`, does nothing else) if `signal` was already
/// aborted — matches real `AbortController.abort()`, which only the first
/// call affects.
pub(super) unsafe fn fire_abort(
    ctx: *mut sys::JSContext,
    signal: sys::JSValue,
    reason: sys::JSValue,
) {
    if signal_aborted(ctx, signal) {
        sys::JS_FreeValue(ctx, reason);
        return;
    }
    set_prop(ctx, signal, AND_ABORTED, sys::js_bool(true));
    set_prop(ctx, signal, AND_REASON, reason);

    let list = get_prop(ctx, signal, AND_LISTENERS);
    if sys::JS_IsArray(list) {
        let mut len = 0;
        sys::JS_GetLength(ctx, list, &mut len);
        for i in 0..len as u32 {
            let record = sys::JS_GetPropertyUint32(ctx, list, i);
            let node = get_prop(ctx, record, b"node\0");
            let callback = get_prop(ctx, record, b"callback\0");
            let type_val = get_prop(ctx, record, b"type\0");
            let capture_val = get_prop(ctx, record, b"capture\0");
            let capture = sys::JS_ToBool(ctx, capture_val) != 0;
            sys::JS_FreeValue(ctx, capture_val);
            if let Some(event_type) = read_js_string(ctx, type_val) {
                events::remove_matching_record(ctx, node, &event_type, callback, capture);
            }
            sys::JS_FreeValue(ctx, type_val);
            sys::JS_FreeValue(ctx, callback);
            sys::JS_FreeValue(ctx, node);
            sys::JS_FreeValue(ctx, record);
        }
        // Every tracked listener has been removed - an empty array keeps
        // a second abort() (already a no-op above) and repeated
        // signal_aborted checks cheap without holding dangling records.
        set_prop(ctx, signal, AND_LISTENERS, sys::JS_NewArray(ctx));
    }
    sys::JS_FreeValue(ctx, list);

    events::dispatch_simple(ctx, signal, "abort", false, false);
}

/// The default `reason` a plain `abort()`/`AbortSignal.abort()` call (no
/// explicit reason argument) uses — a real `Error` (via the global `Error`
/// constructor, same pattern `notifications.rs`'s `permission_error`
/// already uses, since `quickjs-sys` doesn't bind `JS_ThrowTypeError`'s
/// variadic form) with `.name` set to `"AbortError"`. This crate has no
/// `DOMException` class to produce the spec-exact type — a documented
/// substitution, not a silent placeholder (`.name`/`.message` are both
/// real and inspectable).
unsafe fn default_abort_reason(ctx: *mut sys::JSContext) -> sys::JSValue {
    let global = sys::JS_GetGlobalObject(ctx);
    let error_name = CString::new("Error").unwrap();
    let error_ctor = sys::JS_GetPropertyStr(ctx, global, error_name.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let mut message = new_js_string(ctx, "signal is aborted without reason");
    let error = sys::JS_Call(ctx, error_ctor, sys::js_undefined(), 1, &mut message);
    sys::JS_FreeValue(ctx, message);
    sys::JS_FreeValue(ctx, error_ctor);
    set_prop(ctx, error, b"name\0", new_js_string(ctx, "AbortError"));
    error
}

pub(super) unsafe fn resolve_reason(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc >= 1 {
        sys::JS_DupValue(ctx, *argv)
    } else {
        default_abort_reason(ctx)
    }
}

pub(super) unsafe fn make_signal(ctx: *mut sys::JSContext, proto: sys::JSValue) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    set_prop(ctx, obj, AND_ABORTED, sys::js_bool(false));
    set_prop(ctx, obj, AND_REASON, sys::js_undefined());
    obj
}

pub(super) unsafe extern "C" fn signal_constructor(
    ctx: *mut sys::JSContext,
    _new_target: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    // Matches real browsers: `AbortSignal` has no public constructor.
    throw_type_error(ctx, "Illegal constructor")
}

pub(super) unsafe extern "C" fn signal_aborted_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_bool(signal_aborted(ctx, this_val))
}

pub(super) unsafe extern "C" fn signal_reason_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, AND_REASON)
}

pub(super) unsafe extern "C" fn signal_throw_if_aborted(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    if signal_aborted(ctx, this_val) {
        let reason = get_prop(ctx, this_val, AND_REASON);
        sys::JS_Throw(ctx, reason)
    } else {
        sys::js_undefined()
    }
}

pub(super) unsafe extern "C" fn signal_abort_static(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let proto = resolve_prototype(ctx, sys::js_undefined(), "AbortSignal");
    let signal = make_signal(ctx, proto);
    sys::JS_FreeValue(ctx, proto);
    let reason = resolve_reason(ctx, argc, argv);
    set_prop(ctx, signal, AND_ABORTED, sys::js_bool(true));
    set_prop(ctx, signal, AND_REASON, reason);
    signal
}
