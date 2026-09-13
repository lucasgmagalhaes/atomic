//! `Headers`, `Request`, and `Response` classes — the fetch API's
//! core object model. Backed by real per-instance data (not plain
//! `{ok, status, body}` objects), so `fetch()`/`fetchSync()` can
//! return proper `Response` instances whose headers are queryable,
//! bodies consumable via `text()`/`json()`/`arrayBuffer()`, and
//! requests cloneable.
//!
//! Deviations: no `ReadableStream` body (body is eagerly buffered),
//! no `FormData` auto-parse on `Response`, no `cache`/`credentials`/`mode`
//! enforcement (fields exist for API shape but are not acted upon),
//! and `bodyUsed` is not tracked (body methods are safe to call once;
//! calling them twice returns the same data, not an error — a real
//! stream would throw after consumption).
//!
//! Split into one file per class (`headers.rs`/`request.rs`/`response.rs`),
//! this file holding only what's genuinely shared: the small JS-value
//! helpers below and `register`, which just calls each class's own
//! `register`. `Request`/`Response` both need `Headers`' `parse_headers_init`
//! (to accept a `Headers` instance, plain object, or array-of-pairs as
//! their own `headers` init) and `create_headers_object` (their own
//! `.headers` getter returns a fresh `Headers` instance) — both exposed
//! from `headers.rs` and called via `super::headers::...` from the sibling
//! modules, same as any other cross-submodule reference in this crate.

use std::ffi::CString;

use quickjs_sys as sys;

mod headers;
mod headers_init;
mod headers_iterators;
mod request;
mod response;
mod response_body;

pub(crate) use response::create_response_object;

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

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str) -> sys::JSValue {
    let name = CString::new(key).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

unsafe fn throw_type_error(ctx: *mut sys::JSContext, msg: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_js_string(ctx, msg))
}

/// Parses `bytes` as JSON via the real `JS_ParseJSON` C API (not a
/// `JSON.parse` source string built and re-entrantly `JS_Eval`'d from
/// inside a native callback — that path was a real, if rare, source of
/// wrong results under `cargo test`'s per-test spawned threads).
unsafe fn parse_json_bytes(ctx: *mut sys::JSContext, bytes: &[u8]) -> Result<sys::JSValue, String> {
    let mut buf = bytes.to_vec();
    buf.push(0);
    let result = sys::JS_ParseJSON(
        ctx,
        buf.as_ptr() as *const std::os::raw::c_char,
        bytes.len(),
        b"<json>\0".as_ptr() as *const std::os::raw::c_char,
    );
    if result.tag == sys::JS_TAG_EXCEPTION {
        sys::JS_FreeValue(ctx, result);
        let exc = sys::JS_GetException(ctx);
        sys::JS_FreeValue(ctx, exc);
        Err("invalid JSON".to_string())
    } else {
        Ok(result)
    }
}

unsafe fn settled_promise(
    ctx: *mut sys::JSContext,
    result: Result<sys::JSValue, String>,
) -> sys::JSValue {
    let mut resolving_funcs = [sys::js_undefined(); 2];
    let promise = sys::JS_NewPromiseCapability(ctx, resolving_funcs.as_mut_ptr());
    let [resolve, reject] = resolving_funcs;
    let (settle_fn, mut arg) = match result {
        Ok(value) => (resolve, value),
        Err(message) => {
            let name = CString::new("Error").unwrap();
            let global = sys::JS_GetGlobalObject(ctx);
            let error_ctor = sys::JS_GetPropertyStr(ctx, global, name.as_ptr());
            sys::JS_FreeValue(ctx, global);
            let mut msg_arg = new_js_string(ctx, &message);
            let error_obj = sys::JS_Call(ctx, error_ctor, sys::js_undefined(), 1, &mut msg_arg);
            sys::JS_FreeValue(ctx, msg_arg);
            sys::JS_FreeValue(ctx, error_ctor);
            (reject, error_obj)
        }
    };
    let settle_result = sys::JS_Call(ctx, settle_fn, sys::js_undefined(), 1, &mut arg);
    sys::JS_FreeValue(ctx, settle_result);
    sys::JS_FreeValue(ctx, arg);
    sys::JS_FreeValue(ctx, resolve);
    sys::JS_FreeValue(ctx, reject);
    promise
}

/// Registers `Headers`, `Request`, and `Response` as globals.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    headers::register(ctx);
    request::register(ctx);
    response::register(ctx);
}
