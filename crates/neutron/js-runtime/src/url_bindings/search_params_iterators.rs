//! `URLSearchParams`' iterator-shaped methods (`entries`/`keys`/`values`/
//! `forEach`) — split out of `search_params.rs` to keep it under this
//! project's per-file line convention. Still genuinely part of the
//! `URLSearchParams` class, registered onto its prototype by
//! `search_params::register_url_search_params` via
//! `super::search_params_iterators::...` paths.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::js_string;
use super::search_params::sp_opaque;

pub(super) unsafe extern "C" fn sp_entries(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    let arr = sys::JS_NewArray(ctx);
    if inner.is_null() {
        return arr;
    }
    for (i, (k, v)) in (*inner).pairs.iter().enumerate() {
        let pair = sys::JS_NewArray(ctx);
        let k_val = js_string(ctx, k);
        let v_val = js_string(ctx, v);
        sys::JS_SetPropertyUint32(ctx, pair, 0, k_val);
        sys::JS_SetPropertyUint32(ctx, pair, 1, v_val);
        sys::JS_SetPropertyUint32(ctx, arr, i as u32, pair);
    }
    arr
}

pub(super) unsafe extern "C" fn sp_keys(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    let arr = sys::JS_NewArray(ctx);
    if inner.is_null() {
        return arr;
    }
    for (i, (k, _)) in (*inner).pairs.iter().enumerate() {
        let k_val = js_string(ctx, k);
        sys::JS_SetPropertyUint32(ctx, arr, i as u32, k_val);
    }
    arr
}

pub(super) unsafe extern "C" fn sp_values(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    let arr = sys::JS_NewArray(ctx);
    if inner.is_null() {
        return arr;
    }
    for (i, (_, v)) in (*inner).pairs.iter().enumerate() {
        let v_val = js_string(ctx, v);
        sys::JS_SetPropertyUint32(ctx, arr, i as u32, v_val);
    }
    arr
}

pub(super) unsafe extern "C" fn sp_for_each(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    if inner.is_null() || argc < 1 {
        return sys::js_undefined();
    }
    let callback = *argv;
    for (k, v) in &(*inner).pairs {
        let mut args = [js_string(ctx, v), js_string(ctx, k), sys::js_undefined()];
        let result = sys::JS_Call(ctx, callback, this_val, 3, args.as_mut_ptr());
        sys::JS_FreeValue(ctx, result);
        sys::JS_FreeValue(ctx, args[0]);
        sys::JS_FreeValue(ctx, args[1]);
    }
    sys::js_undefined()
}
