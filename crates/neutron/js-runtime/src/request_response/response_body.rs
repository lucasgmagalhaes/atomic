//! `Response`'s body-consumption methods (`text`/`json`/`arrayBuffer`) —
//! split out of `response.rs` to keep it under this project's per-file
//! line convention. Still genuinely part of the `Response` class,
//! registered onto its prototype by `response::register` via
//! `super::response_body::...` paths.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::response::response_opaque;
use super::{new_js_string, parse_json_bytes, settled_promise};

pub(super) unsafe extern "C" fn response_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return settled_promise(ctx, Ok(new_js_string(ctx, "")));
    }
    match &(*ptr).body {
        Some(bytes) => {
            let s = String::from_utf8_lossy(bytes).into_owned();
            settled_promise(ctx, Ok(new_js_string(ctx, &s)))
        }
        None => settled_promise(ctx, Ok(new_js_string(ctx, ""))),
    }
}

pub(super) unsafe extern "C" fn response_json(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return settled_promise(ctx, Err("body is null".to_string()));
    }
    match &(*ptr).body {
        Some(bytes) => match parse_json_bytes(ctx, bytes) {
            Ok(value) => settled_promise(ctx, Ok(value)),
            Err(message) => settled_promise(ctx, Err(message)),
        },
        None => settled_promise(ctx, Err("body is null".to_string())),
    }
}

pub(super) unsafe extern "C" fn response_array_buffer(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return settled_promise(
            ctx,
            Ok(sys::JS_NewArrayBufferCopy(ctx, std::ptr::null(), 0)),
        );
    }
    match &(*ptr).body {
        Some(bytes) => {
            let buf = sys::JS_NewArrayBufferCopy(ctx, bytes.as_ptr(), bytes.len());
            settled_promise(ctx, Ok(buf))
        }
        None => settled_promise(
            ctx,
            Ok(sys::JS_NewArrayBufferCopy(ctx, std::ptr::null(), 0)),
        ),
    }
}
