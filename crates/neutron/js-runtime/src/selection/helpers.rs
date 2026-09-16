//! Shared JS string/property helpers and `resolve_prototype` — split out
//! from `selection/mod.rs`. Same shape as `abort_controller/helpers.rs`
//! (this crate's own convention: a small per-module copy rather than a
//! shared utility, since these two families have no other reason to
//! depend on each other).

use std::ffi::CString;

use quickjs_sys as sys;

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

pub(super) const AND_START_NODE: &[u8] = b"__start_node\0";
pub(super) const AND_START_OFFSET: &[u8] = b"__start_offset\0";
pub(super) const AND_END_NODE: &[u8] = b"__end_node\0";
pub(super) const AND_END_OFFSET: &[u8] = b"__end_offset\0";
pub(super) const AND_RANGE: &[u8] = b"__range\0";

/// Resolves the prototype a new instance should use — `new_target`'s own
/// `.prototype` when present (a real `new Foo()`/subclass call), otherwise
/// `globalThis.<fallback_ctor_name>.prototype`. Same helper
/// `abort_controller/helpers.rs`'s own `resolve_prototype` already
/// established for exactly this purpose.
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
