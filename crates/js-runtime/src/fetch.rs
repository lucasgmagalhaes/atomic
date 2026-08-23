//! `fetchSync(url)`: a synchronous global backed by `net::get`.
//!
//! Deviation from the spec: real `fetch` is Promise-based and non-blocking.
//! There's no event loop or microtask queue in this runtime yet (same gap
//! documented for `performance.now`'s per-document origin and timers/rAF —
//! all deferred to phase 4's `profile`/`ipc` process, where a real per-tab
//! run loop will exist to drive them). Until then this blocks the calling
//! thread on the request and returns a plain result object instead of a
//! `Promise`: `{ ok, status, body }`, where `body` is the response bytes
//! decoded as UTF-8 (lossily — binary responses aren't representable
//! without `ArrayBuffer`/`Uint8Array` support on the return path, which
//! doesn't exist here). On network/TLS/URL failure, returns
//! `{ ok: false, status: 0, body: "" }` with the error swallowed rather
//! than thrown — no exception-construction helper is bound yet either.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

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

unsafe fn set_str(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: &str) {
    let name = CString::new(key).unwrap();
    let js_val = new_js_string(ctx, val);
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), js_val);
}

unsafe fn set_num(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: f64) {
    let name = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), sys::js_float64(val));
}

unsafe fn set_bool(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: bool) {
    let name = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), sys::js_bool(val));
}

unsafe extern "C" fn fetch_sync(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let result = sys::JS_NewObject(ctx);

    if argc < 1 {
        set_bool(ctx, result, "ok", false);
        set_num(ctx, result, "status", 0.0);
        set_str(ctx, result, "body", "");
        return result;
    }

    let Some(url) = read_js_string(ctx, *argv) else {
        set_bool(ctx, result, "ok", false);
        set_num(ctx, result, "status", 0.0);
        set_str(ctx, result, "body", "");
        return result;
    };

    match net::get(&url) {
        Ok(response) => {
            let body = String::from_utf8_lossy(&response.body).into_owned();
            set_bool(ctx, result, "ok", (200..300).contains(&response.status));
            set_num(ctx, result, "status", response.status as f64);
            set_str(ctx, result, "body", &body);
        }
        Err(_) => {
            set_bool(ctx, result, "ok", false);
            set_num(ctx, result, "status", 0.0);
            set_str(ctx, result, "body", "");
        }
    }

    result
}

/// Registers `fetchSync` as a global on `ctx`.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);

    let name = CString::new("fetchSync").unwrap();
    let f = sys::JS_NewCFunction2(ctx, fetch_sync, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, global, name.as_ptr(), f);

    sys::JS_FreeValue(ctx, global);
}
