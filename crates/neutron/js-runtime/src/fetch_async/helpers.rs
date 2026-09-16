//! Shared string/property/callback helpers — split out from
//! `fetch_async/mod.rs`.

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

pub(super) unsafe fn throw_type_error(ctx: *mut sys::JSContext, message: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_js_string(ctx, message))
}

pub(super) unsafe fn set_str(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: &str) {
    let name = CString::new(key).unwrap();
    let js_val = new_js_string(ctx, val);
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), js_val);
}

pub(super) unsafe fn set_num(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: f64) {
    let name = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), sys::js_float64(val));
}

pub(super) unsafe fn get_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    key: &str,
) -> sys::JSValue {
    let name = CString::new(key).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

/// Calls `callback` with `this = this_val` and one argument, if
/// `callback` is anything other than `undefined` - no `JS_IsFunction`
/// binding exists yet, so a non-function value just fails the call and
/// the resulting exception value is discarded, same "no exception
/// plumbing yet" convention used elsewhere in this crate (e.g. `timers`'s
/// callback firing). `JS_Call`'s `argv` is `JSValueConst*` - borrowed, not
/// consumed - so `arg` (and `callback` itself, also borrowed as
/// `func_obj`) must be freed here regardless of whether the call actually
/// happened.
pub(super) unsafe fn call_if_present(
    ctx: *mut sys::JSContext,
    callback: sys::JSValue,
    this_val: sys::JSValue,
    mut arg: sys::JSValue,
) {
    if callback.tag != sys::JS_TAG_UNDEFINED {
        let result = sys::JS_Call(ctx, callback, this_val, 1, &mut arg);
        sys::JS_FreeValue(ctx, result);
    }
    sys::JS_FreeValue(ctx, arg);
    sys::JS_FreeValue(ctx, callback);
}

pub(super) unsafe fn build_response_object(
    ctx: *mut sys::JSContext,
    response: &net::Response,
    url: &str,
) -> sys::JSValue {
    crate::request_response::create_response_object(
        ctx,
        response.status,
        &response.headers,
        Some(response.body.clone()),
        url,
    )
}
