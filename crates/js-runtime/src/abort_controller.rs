//! `AbortController`/`AbortSignal` (`ROADMAP.md` P2 item 17): real
//! cancellation-signaling objects, wired into `addEventListener`'s
//! previously-no-op `signal` option (`events::listeners`).
//!
//! Neither class stores native/opaque state — both are plain `JS_NewObject`
//! instances with a real prototype (`JS_SetPrototype`, resolved from the
//! constructor's own `.prototype`, same convention `blob.rs`'s
//! `resolve_prototype` established) carrying hidden `__aborted`/`__reason`/
//! `__signal_listeners` own properties, same "state as plain JS values on
//! the object" convention `history.rs`'s `__entries`/`__index` and
//! `listeners.rs`'s per-record objects already use. `AbortSignal` reuses
//! [`crate::events::define_simple_event_target`] for its `addEventListener`/
//! `dispatchEvent("abort")` surface — it's exactly the "plain JS object,
//! not a `Node`" shape that helper already exists for (`window`/`document`/
//! `history`).
//!
//! `signal` tracking for `addEventListener`: rather than a native closure
//! (this crate's `extern "C" fn` callbacks can't capture Rust state),
//! `track_listener` pushes a plain `{node, type, callback, capture}` record
//! onto the signal's own `__signal_listeners` array — the same shape
//! `listeners.rs`'s own per-`(node, type)` records already use. `fire_abort`
//! walks that array and calls `events::remove_matching_record` for each
//! entry, so a listener registered with `{signal}` is really removed when
//! the controller aborts, not just marked.
//!
//! Real scope cuts (documented, not silent): `new AbortSignal()` throws
//! (matches real browsers — only `AbortSignal.abort()`/an `AbortController`
//! ever produce one); no `AbortSignal.timeout()`/`AbortSignal.any()`; no
//! `.onabort` IDL attribute (only `addEventListener("abort", ...)` — this
//! crate has no generic `on<type>` property convention for any
//! `EventTarget`, not one newly cut here); and `fetch`/`XMLHttpRequest`
//! don't yet read a `signal` option themselves (`ROADMAP.md`'s explicit
//! call-out was `addEventListener`'s `signal`, wired here — request
//! cancellation is a separate, larger follow-up since this crate's
//! in-flight fetches run on a real OS thread with no cancellation channel
//! yet).
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::events;

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

unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &[u8]) -> sys::JSValue {
    sys::JS_GetPropertyStr(ctx, obj, key.as_ptr() as *const _)
}

unsafe fn set_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &[u8], val: sys::JSValue) {
    sys::JS_SetPropertyStr(ctx, obj, key.as_ptr() as *const _, val);
}

unsafe fn throw_type_error(ctx: *mut sys::JSContext, message: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_js_string(ctx, message))
}

const AND_ABORTED: &[u8] = b"__aborted\0";
const AND_REASON: &[u8] = b"__reason\0";
const AND_LISTENERS: &[u8] = b"__signal_listeners\0";
const AND_SIGNAL: &[u8] = b"__signal\0";

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
unsafe fn fire_abort(ctx: *mut sys::JSContext, signal: sys::JSValue, reason: sys::JSValue) {
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

unsafe fn resolve_reason(
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

/// Resolves the prototype a new instance should use — `new_target`'s own
/// `.prototype` when present (a real `new Foo()`/subclass call), otherwise
/// `globalThis.<fallback_ctor_name>.prototype`. Same helper `blob.rs`
/// already established for exactly this purpose (see its own doc for why
/// the explicit lookup, not `JS_NewObjectClass`'s implicit default, is
/// used) — this module doesn't need a native class at all (no opaque
/// per-instance data), so it skips `JS_NewObjectClass`/`class_registry`
/// entirely and just sets the prototype on a plain `JS_NewObject`.
unsafe fn resolve_prototype(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    fallback_ctor_name: &str,
) -> sys::JSValue {
    if new_target.tag != sys::JS_TAG_UNDEFINED {
        let proto = get_prop(ctx, new_target, b"prototype\0");
        if proto.tag != sys::JS_TAG_UNDEFINED {
            return proto;
        }
        sys::JS_FreeValue(ctx, proto);
    }
    let global = sys::JS_GetGlobalObject(ctx);
    let ctor_name = CString::new(fallback_ctor_name).unwrap();
    let ctor = sys::JS_GetPropertyStr(ctx, global, ctor_name.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let proto = get_prop(ctx, ctor, b"prototype\0");
    sys::JS_FreeValue(ctx, ctor);
    proto
}

unsafe fn make_signal(ctx: *mut sys::JSContext, proto: sys::JSValue) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    set_prop(ctx, obj, AND_ABORTED, sys::js_bool(false));
    set_prop(ctx, obj, AND_REASON, sys::js_undefined());
    obj
}

unsafe extern "C" fn signal_constructor(
    ctx: *mut sys::JSContext,
    _new_target: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    // Matches real browsers: `AbortSignal` has no public constructor.
    throw_type_error(ctx, "Illegal constructor")
}

unsafe extern "C" fn signal_aborted_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_bool(signal_aborted(ctx, this_val))
}

unsafe extern "C" fn signal_reason_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, AND_REASON)
}

unsafe extern "C" fn signal_throw_if_aborted(
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

unsafe extern "C" fn signal_abort_static(
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

unsafe extern "C" fn controller_constructor(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let proto = resolve_prototype(ctx, new_target, "AbortController");
    let obj = sys::JS_NewObject(ctx);
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);
    let signal_proto = resolve_prototype(ctx, sys::js_undefined(), "AbortSignal");
    let signal = make_signal(ctx, signal_proto);
    sys::JS_FreeValue(ctx, signal_proto);
    set_prop(ctx, obj, AND_SIGNAL, signal);
    obj
}

unsafe extern "C" fn controller_signal_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, AND_SIGNAL)
}

unsafe extern "C" fn controller_abort(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let signal = get_prop(ctx, this_val, AND_SIGNAL);
    if signal.tag == sys::JS_TAG_OBJECT {
        let reason = resolve_reason(ctx, argc, argv);
        fire_abort(ctx, signal, reason);
    }
    sys::JS_FreeValue(ctx, signal);
    sys::js_undefined()
}

/// Registers the global `AbortController` and `AbortSignal` constructors.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);

    let signal_proto = sys::JS_NewObject(ctx);
    crate::js_helpers::define_getter(ctx, signal_proto, "aborted", signal_aborted_get);
    crate::js_helpers::define_getter(ctx, signal_proto, "reason", signal_reason_get);
    crate::js_helpers::define_method(
        ctx,
        signal_proto,
        "throwIfAborted",
        signal_throw_if_aborted,
        0,
    );
    events::define_simple_event_target(ctx, signal_proto);

    let signal_ctor_name = CString::new("AbortSignal").unwrap();
    let signal_ctor = sys::JS_NewCFunction2(
        ctx,
        signal_constructor,
        signal_ctor_name.as_ptr(),
        0,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    crate::js_helpers::define_method(ctx, signal_ctor, "abort", signal_abort_static, 1);
    let proto_name = CString::new("prototype").unwrap();
    // Consumes `signal_proto` directly (no `JS_DupValue`) — this is its
    // only owning reference (unlike `blob.rs`'s `ensure_blob_class`, there's
    // no `JS_SetClassProto` here to hand a second one to; this crate skips
    // `class_registry` entirely for these two classes, see the module doc).
    sys::JS_SetPropertyStr(ctx, signal_ctor, proto_name.as_ptr(), signal_proto);
    sys::JS_SetPropertyStr(ctx, global, signal_ctor_name.as_ptr(), signal_ctor);

    let controller_proto = sys::JS_NewObject(ctx);
    crate::js_helpers::define_getter(ctx, controller_proto, "signal", controller_signal_get);
    crate::js_helpers::define_method(ctx, controller_proto, "abort", controller_abort, 0);

    let controller_ctor_name = CString::new("AbortController").unwrap();
    let controller_ctor = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<
            unsafe extern "C" fn(
                *mut sys::JSContext,
                sys::JSValue,
                c_int,
                *mut sys::JSValue,
            ) -> sys::JSValue,
            sys::JSCFunction,
        >(controller_constructor),
        controller_ctor_name.as_ptr(),
        0,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    // Same reasoning as `signal_proto` above — consumed directly.
    sys::JS_SetPropertyStr(ctx, controller_ctor, proto_name.as_ptr(), controller_proto);
    sys::JS_SetPropertyStr(ctx, global, controller_ctor_name.as_ptr(), controller_ctor);

    sys::JS_FreeValue(ctx, global);
}
