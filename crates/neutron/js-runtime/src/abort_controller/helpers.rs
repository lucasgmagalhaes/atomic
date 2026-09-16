//! Shared JS string/property helpers and `resolve_prototype` — split out
//! from `abort_controller.rs`.

use std::ffi::CString;

use quickjs_sys as sys;

pub(super) unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
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

pub(super) unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

pub(super) unsafe fn get_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    key: &[u8],
) -> sys::JSValue {
    sys::JS_GetPropertyStr(ctx, obj, key.as_ptr() as *const _)
}

pub(super) unsafe fn set_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    key: &[u8],
    val: sys::JSValue,
) {
    sys::JS_SetPropertyStr(ctx, obj, key.as_ptr() as *const _, val);
}

pub(super) unsafe fn throw_type_error(ctx: *mut sys::JSContext, message: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_js_string(ctx, message))
}

pub(super) const AND_ABORTED: &[u8] = b"__aborted\0";
pub(super) const AND_REASON: &[u8] = b"__reason\0";
pub(super) const AND_LISTENERS: &[u8] = b"__signal_listeners\0";
pub(super) const AND_SIGNAL: &[u8] = b"__signal\0";

/// Resolves the prototype a new instance should use — `new_target`'s own
/// `.prototype` when present (a real `new Foo()`/subclass call), otherwise
/// `globalThis.<fallback_ctor_name>.prototype`. Same helper `blob.rs`
/// already established for exactly this purpose (see its own doc for why
/// the explicit lookup, not `JS_NewObjectClass`'s implicit default, is
/// used) — this module doesn't need a native class at all (no opaque
/// per-instance data), so it skips `JS_NewObjectClass`/`class_registry`
/// entirely and just sets the prototype on a plain `JS_NewObject`.
pub(super) unsafe fn resolve_prototype(
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
