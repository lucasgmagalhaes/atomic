//! `Blob.prototype.slice()`/`text()`/`arrayBuffer()` — split out from
//! `blob.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::blob_opaque;
use super::construct::{make_blob_object, resolve_prototype};
use super::helpers::{new_js_string, read_js_number, read_js_string};

pub(super) unsafe extern "C" fn blob_slice(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = blob_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let total = (*ptr).bytes.len() as i64;
    let start = if argc >= 1 {
        read_js_number(*argv).map(|n| n as i64).unwrap_or(0)
    } else {
        0
    };
    let end = if argc >= 2 {
        read_js_number(*argv.add(1))
            .map(|n| n as i64)
            .unwrap_or(total)
    } else {
        total
    };
    let start = start.clamp(0, total) as usize;
    let end = end.clamp(0, total) as usize;
    let sliced = if start < end {
        (&(*ptr).bytes)[start..end].to_vec()
    } else {
        Vec::new()
    };

    let mime = if argc >= 3 {
        read_js_string(ctx, *argv.add(2)).unwrap_or_default()
    } else {
        (*ptr).mime.clone()
    };
    // Per spec, `slice()` always returns a `Blob`, never a `File` -
    // `resolve_prototype` with `new_target: undefined` always takes the
    // fallback (`globalThis.Blob.prototype`) branch.
    let proto = resolve_prototype(ctx, sys::js_undefined(), "Blob");
    let obj = make_blob_object(ctx, proto, sliced, mime);
    sys::JS_FreeValue(ctx, proto);
    obj
}

unsafe fn resolved_promise(ctx: *mut sys::JSContext, value: sys::JSValue) -> sys::JSValue {
    let mut resolving_funcs = [sys::js_undefined(); 2];
    let promise = sys::JS_NewPromiseCapability(ctx, resolving_funcs.as_mut_ptr());
    let [resolve, reject] = resolving_funcs;
    let mut arg = value;
    let result = sys::JS_Call(ctx, resolve, sys::js_undefined(), 1, &mut arg);
    sys::JS_FreeValue(ctx, result);
    sys::JS_FreeValue(ctx, value);
    sys::JS_FreeValue(ctx, resolve);
    sys::JS_FreeValue(ctx, reject);
    promise
}

pub(super) unsafe extern "C" fn blob_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = blob_opaque(sys::JS_GetRuntime(ctx), this_val);
    let text = if ptr.is_null() {
        String::new()
    } else {
        String::from_utf8_lossy(&(*ptr).bytes).into_owned()
    };
    let js_text = new_js_string(ctx, &text);
    resolved_promise(ctx, js_text)
}

pub(super) unsafe extern "C" fn blob_array_buffer(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = blob_opaque(sys::JS_GetRuntime(ctx), this_val);
    let buf = if ptr.is_null() {
        sys::JS_NewArrayBufferCopy(ctx, std::ptr::null(), 0)
    } else {
        sys::JS_NewArrayBufferCopy(ctx, (*ptr).bytes.as_ptr(), (*ptr).bytes.len())
    };
    resolved_promise(ctx, buf)
}
