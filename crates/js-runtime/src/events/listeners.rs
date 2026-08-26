//! `addEventListener`/`removeEventListener`: per-`(node, type)` listener
//! storage as plain JS values (a `{callback, capture, once, passive}`
//! record per registration, no extra native class), plus the shared
//! per-record helpers [`super::dispatch`] reuses.

use quickjs_sys as sys;
use std::ffi::CString;
use std::os::raw::c_int;

use super::util::{read_string, type_error};

const LISTENERS_PROP: &[u8] = b"__listeners\0";
const MAX_LISTENERS_PER_TYPE: i64 = 64;

pub(super) unsafe fn listeners(ctx: *mut sys::JSContext, node: sys::JSValue) -> sys::JSValue {
    let existing = sys::JS_GetPropertyStr(ctx, node, LISTENERS_PROP.as_ptr() as *const _);
    if existing.tag != sys::JS_TAG_UNDEFINED {
        return existing;
    }
    sys::JS_FreeValue(ctx, existing);
    let created = sys::JS_NewObject(ctx);
    sys::JS_SetPropertyStr(
        ctx,
        node,
        LISTENERS_PROP.as_ptr() as *const _,
        sys::JS_DupValue(ctx, created),
    );
    created
}

/// Real `EventListener` shape per spec: either a plain callable, or an
/// object exposing a callable `handleEvent` method (invoked with the
/// listener object itself as `this`, per spec — not the node).
unsafe fn is_valid_listener(ctx: *mut sys::JSContext, value: sys::JSValue) -> bool {
    if sys::JS_IsFunction(ctx, value) {
        return true;
    }
    if value.tag != sys::JS_TAG_OBJECT {
        return false;
    }
    let name = CString::new("handleEvent").unwrap();
    let handler = sys::JS_GetPropertyStr(ctx, value, name.as_ptr());
    let ok = sys::JS_IsFunction(ctx, handler);
    sys::JS_FreeValue(ctx, handler);
    ok
}

/// Real `addEventListener`/`removeEventListener` third-argument shape:
/// either a bare `useCapture` boolean, or an options object with
/// `capture`/`once`/`passive`. `signal` (`AbortSignal`) isn't read — this
/// crate has no `AbortController`/`AbortSignal` implementation yet, a
/// documented scope cut rather than a silent no-op (nothing here claims to
/// honor it).
pub(super) struct ListenerOptions {
    pub(super) capture: bool,
    pub(super) once: bool,
    pub(super) passive: bool,
}

pub(super) unsafe fn read_listener_options(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
    index: usize,
) -> ListenerOptions {
    if argc <= index as c_int {
        return ListenerOptions {
            capture: false,
            once: false,
            passive: false,
        };
    }
    let value = *argv.add(index);
    if value.tag != sys::JS_TAG_OBJECT {
        return ListenerOptions {
            capture: sys::JS_ToBool(ctx, value) != 0,
            once: false,
            passive: false,
        };
    }
    let read = |name: &str| {
        let cname = CString::new(name).unwrap();
        let prop = sys::JS_GetPropertyStr(ctx, value, cname.as_ptr());
        let b = sys::JS_ToBool(ctx, prop) != 0;
        sys::JS_FreeValue(ctx, prop);
        b
    };
    ListenerOptions {
        capture: read("capture"),
        once: read("once"),
        passive: read("passive"),
    }
}

const RECORD_CALLBACK: &[u8] = b"callback\0";
const RECORD_CAPTURE: &[u8] = b"capture\0";
const RECORD_ONCE: &[u8] = b"once\0";
const RECORD_PASSIVE: &[u8] = b"passive\0";

/// Wraps one registered listener with its options into a plain JS object —
/// this crate stores listener state as native QuickJS values (no extra
/// native class), so a record is just `{callback, capture, once, passive}`
/// pushed into the same per-type array `dispatch_at`/`remove` already used
/// for bare callbacks.
unsafe fn make_record(
    ctx: *mut sys::JSContext,
    callback: sys::JSValue,
    options: &ListenerOptions,
) -> sys::JSValue {
    let record = sys::JS_NewObject(ctx);
    sys::JS_SetPropertyStr(
        ctx,
        record,
        RECORD_CALLBACK.as_ptr() as *const _,
        sys::JS_DupValue(ctx, callback),
    );
    sys::JS_SetPropertyStr(
        ctx,
        record,
        RECORD_CAPTURE.as_ptr() as *const _,
        sys::js_bool(options.capture),
    );
    sys::JS_SetPropertyStr(
        ctx,
        record,
        RECORD_ONCE.as_ptr() as *const _,
        sys::js_bool(options.once),
    );
    sys::JS_SetPropertyStr(
        ctx,
        record,
        RECORD_PASSIVE.as_ptr() as *const _,
        sys::js_bool(options.passive),
    );
    record
}

pub(super) unsafe fn record_callback(
    ctx: *mut sys::JSContext,
    record: sys::JSValue,
) -> sys::JSValue {
    sys::JS_GetPropertyStr(ctx, record, RECORD_CALLBACK.as_ptr() as *const _)
}

pub(super) unsafe fn record_bool(
    ctx: *mut sys::JSContext,
    record: sys::JSValue,
    name: &[u8],
) -> bool {
    let value = sys::JS_GetPropertyStr(ctx, record, name.as_ptr() as *const _);
    let result = sys::JS_ToBool(ctx, value) != 0;
    sys::JS_FreeValue(ctx, value);
    result
}

pub(super) unsafe extern "C" fn add(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return sys::js_undefined();
    }
    let Some(kind) = read_string(ctx, *argv) else {
        return type_error(ctx, "event type must be a string");
    };
    let callback = *argv.add(1);
    if !is_valid_listener(ctx, callback) {
        return type_error(
            ctx,
            "event listener must be a function or an object with handleEvent",
        );
    }
    let options = read_listener_options(ctx, argc, argv, 2);
    let all = listeners(ctx, node);
    let name = CString::new(kind).unwrap_or_default();
    let old = sys::JS_GetPropertyStr(ctx, all, name.as_ptr());
    let list = if sys::JS_IsArray(old) {
        old
    } else {
        sys::JS_FreeValue(ctx, old);
        let a = sys::JS_NewArray(ctx);
        sys::JS_SetPropertyStr(ctx, all, name.as_ptr(), sys::JS_DupValue(ctx, a));
        a
    };
    let mut len = 0;
    sys::JS_GetLength(ctx, list, &mut len);
    if len >= MAX_LISTENERS_PER_TYPE {
        sys::JS_FreeValue(ctx, list);
        sys::JS_FreeValue(ctx, all);
        return type_error(ctx, "event listener limit exceeded");
    }
    let record = make_record(ctx, callback, &options);
    sys::JS_SetPropertyUint32(ctx, list, len as u32, record);
    sys::JS_FreeValue(ctx, list);
    sys::JS_FreeValue(ctx, all);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn remove(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let Some(kind) = read_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let all = listeners(ctx, node);
    let name = CString::new(kind).unwrap_or_default();
    if argc < 2 {
        sys::JS_SetPropertyStr(ctx, all, name.as_ptr(), sys::js_undefined());
    } else {
        let list = sys::JS_GetPropertyStr(ctx, all, name.as_ptr());
        if sys::JS_IsArray(list) {
            let callback = *argv.add(1);
            let capture = read_listener_options(ctx, argc, argv, 2).capture;
            let mut len = 0;
            sys::JS_GetLength(ctx, list, &mut len);
            for i in 0..len as u32 {
                let record = sys::JS_GetPropertyUint32(ctx, list, i);
                if record.tag == sys::JS_TAG_UNDEFINED {
                    sys::JS_FreeValue(ctx, record);
                    continue;
                }
                let registered = record_callback(ctx, record);
                let matches = sys::JS_IsStrictEqual(ctx, registered, callback)
                    && record_bool(ctx, record, RECORD_CAPTURE) == capture;
                sys::JS_FreeValue(ctx, registered);
                sys::JS_FreeValue(ctx, record);
                if matches {
                    sys::JS_SetPropertyUint32(ctx, list, i, sys::js_undefined());
                    break;
                }
            }
        }
        sys::JS_FreeValue(ctx, list)
    }
    sys::JS_FreeValue(ctx, all);
    sys::js_undefined()
}

/// Removes the first record in `node`'s `event_type` list matching
/// `callback`+`capture` — used to drop a `once` listener right after it
/// fires. Sets the slot to `undefined` in place (same sparse-array
/// convention `remove()` already uses) rather than splicing, so this never
/// shifts indices a concurrent iteration snapshot elsewhere still relies on.
pub(super) unsafe fn remove_matching_record(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    event_type: &str,
    callback: sys::JSValue,
    capture: bool,
) {
    let all = listeners(ctx, node);
    let name = CString::new(event_type).unwrap_or_default();
    let list = sys::JS_GetPropertyStr(ctx, all, name.as_ptr());
    sys::JS_FreeValue(ctx, all);
    if sys::JS_IsArray(list) {
        let mut len = 0;
        sys::JS_GetLength(ctx, list, &mut len);
        for i in 0..len as u32 {
            let record = sys::JS_GetPropertyUint32(ctx, list, i);
            if record.tag == sys::JS_TAG_UNDEFINED {
                sys::JS_FreeValue(ctx, record);
                continue;
            }
            let registered = record_callback(ctx, record);
            let matches = sys::JS_IsStrictEqual(ctx, registered, callback)
                && record_bool(ctx, record, RECORD_CAPTURE) == capture;
            sys::JS_FreeValue(ctx, registered);
            sys::JS_FreeValue(ctx, record);
            if matches {
                sys::JS_SetPropertyUint32(ctx, list, i, sys::js_undefined());
                break;
            }
        }
    }
    sys::JS_FreeValue(ctx, list);
}
