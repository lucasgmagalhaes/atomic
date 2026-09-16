//! `URL` constructor and `URLSearchParams` — real URL parsing/resolution
//! backed by the `url` crate (already a dependency for `location` and
//! `cors`). `URLSearchParams` is a full key-value bag with multiple
//! values per key, matching the real API's `append`/`delete`/`get`/
//! `getAll`/`has`/`set`/`sort`/`toString` plus iteration.
//!
//! Deviations: `URLSearchParams` iterator methods return plain arrays
//! (no `Symbol.iterator` support in quickjs-sys); `URL` constructor
//! rejects relative URLs without a base (matching the real spec);
//! `URL.href` setter re-parses the full string (real URL mutation).
//!
//! Split into `search_params.rs`/`search_params_iterators.rs` (the
//! `URLSearchParams` class) and `url.rs`/`url_parts.rs` (the `URL`
//! class) — `URL` genuinely depends on `URLSearchParams`'s own opaque
//! data (a `URL` instance stores its query as a real `_sp` property, an
//! actual `URLSearchParams` object, not a separate parallel
//! representation), hence the cross-module `pub(super)` surface between
//! them.
#![allow(dead_code)]

use std::ffi::CString;

use quickjs_sys as sys;

mod search_params;
mod search_params_iterators;
mod search_params_methods;
mod url;
mod url_constructor;
mod url_parts;

unsafe fn js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}

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

unsafe fn get_property(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str) -> sys::JSValue {
    let c = CString::new(key).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, c.as_ptr())
}

unsafe fn set_property(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: sys::JSValue) {
    let c = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, c.as_ptr(), val);
}

/// Build a TypeError and throw it via JS_Throw.
unsafe fn type_error(ctx: *mut sys::JSContext, msg: &str) -> sys::JSValue {
    let msg_val = js_string(ctx, msg);
    let name = CString::new("TypeError").unwrap();
    let global = sys::JS_GetGlobalObject(ctx);
    let error_ctor = sys::JS_GetPropertyStr(ctx, global, name.as_ptr());
    let mut arg = msg_val;
    let error = sys::JS_Call(ctx, error_ctor, sys::js_undefined(), 1, &mut arg);
    sys::JS_FreeValue(ctx, msg_val);
    sys::JS_FreeValue(ctx, error_ctor);
    sys::JS_FreeValue(ctx, global);
    sys::JS_Throw(ctx, error);
    sys::js_exception()
}

/// Registers `URL`, `URLSearchParams` as globals.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    search_params::register_url_search_params(ctx);
    url::register_url(ctx);
}
