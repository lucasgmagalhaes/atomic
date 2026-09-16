//! `Headers`' iterator-shaped methods (`entries`/`keys`/`values`/
//! `forEach`) — split out of `headers.rs` purely to keep that file under
//! this project's per-file line convention; these are still genuinely
//! part of the `Headers` class, registered onto its prototype by
//! `headers::register` via `super::headers_iterators::...` paths.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::headers::headers_opaque;
use super::new_js_string;

pub(super) unsafe extern "C" fn headers_entries(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let entries = &(*ptr).entries;
    let len = entries.len() as u32;
    let arr = sys::JS_NewArray(ctx);
    for i in 0..len {
        let pair = sys::JS_NewArray(ctx);
        let (ref name, ref value) = entries[i as usize];
        sys::JS_SetPropertyUint32(ctx, pair, 0, new_js_string(ctx, name));
        sys::JS_SetPropertyUint32(ctx, pair, 1, new_js_string(ctx, value));
        sys::JS_SetPropertyUint32(ctx, arr, i, pair);
    }
    arr
}

pub(super) unsafe extern "C" fn headers_keys(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let entries = &(*ptr).entries;
    let len = entries.len() as u32;
    let arr = sys::JS_NewArray(ctx);
    for i in 0..len {
        let val = new_js_string(ctx, &entries[i as usize].0);
        sys::JS_SetPropertyUint32(ctx, arr, i, val);
    }
    arr
}

pub(super) unsafe extern "C" fn headers_values(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let entries = &(*ptr).entries;
    let len = entries.len() as u32;
    let arr = sys::JS_NewArray(ctx);
    for i in 0..len {
        let val = new_js_string(ctx, &entries[i as usize].1);
        sys::JS_SetPropertyUint32(ctx, arr, i, val);
    }
    arr
}

pub(super) unsafe extern "C" fn headers_for_each(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() || argc < 1 {
        return sys::js_undefined();
    }
    let callback = *argv;
    if !sys::JS_IsFunction(ctx, callback) {
        return sys::js_undefined();
    }
    let entries = &(*ptr).entries;
    for (name, value) in entries {
        let mut args = [
            new_js_string(ctx, value),
            new_js_string(ctx, name),
            this_val,
        ];
        let result = sys::JS_Call(ctx, callback, this_val, 3, args.as_mut_ptr());
        sys::JS_FreeValue(ctx, result);
        sys::JS_FreeValue(ctx, args[0]);
        sys::JS_FreeValue(ctx, args[1]);
    }
    sys::js_undefined()
}
