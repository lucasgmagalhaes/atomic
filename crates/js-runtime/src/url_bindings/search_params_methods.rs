//! `URLSearchParams`' mutation/read methods (`append`/`delete`/`get`/
//! `getAll`/`has`/`set`/`sort`/`toString`/`length`) — split out of
//! `search_params.rs` to keep it under this project's per-file line
//! convention. Still genuinely part of the `URLSearchParams` class,
//! registered onto its prototype by
//! `search_params::register_url_search_params` via
//! `super::search_params_methods::...` paths.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::search_params::{build_search_string, sp_opaque};
use super::{js_string, read_js_string};

pub(super) unsafe extern "C" fn sp_append(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    if inner.is_null() || argc < 2 {
        return sys::js_undefined();
    }
    let key = read_js_string(ctx, *argv).unwrap_or_default();
    let val = read_js_string(ctx, *argv.add(1)).unwrap_or_default();
    (*inner).pairs.push((key, val));
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn sp_delete(
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
    let key = read_js_string(ctx, *argv).unwrap_or_default();
    (*inner).pairs.retain(|(k, _)| k != &key);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn sp_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    if inner.is_null() || argc < 1 {
        return sys::js_null();
    }
    let key = read_js_string(ctx, *argv).unwrap_or_default();
    match (*inner).pairs.iter().find(|(k, _)| k == &key) {
        Some((_, v)) => js_string(ctx, v),
        None => sys::js_null(),
    }
}

pub(super) unsafe extern "C" fn sp_get_all(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    let arr = sys::JS_NewArray(ctx);
    if inner.is_null() || argc < 1 {
        return arr;
    }
    let key = read_js_string(ctx, *argv).unwrap_or_default();
    let mut i = 0u32;
    for (_, v) in (*inner).pairs.iter().filter(|(k, _)| k == &key) {
        let v_val = js_string(ctx, v);
        sys::JS_SetPropertyUint32(ctx, arr, i, v_val);
        i += 1;
    }
    arr
}

pub(super) unsafe extern "C" fn sp_has(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    if inner.is_null() || argc < 1 {
        return sys::js_bool(false);
    }
    let key = read_js_string(ctx, *argv).unwrap_or_default();
    sys::js_bool((*inner).pairs.iter().any(|(k, _)| k == &key))
}

pub(super) unsafe extern "C" fn sp_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    if inner.is_null() || argc < 2 {
        return sys::js_undefined();
    }
    let key = read_js_string(ctx, *argv).unwrap_or_default();
    let val = read_js_string(ctx, *argv.add(1)).unwrap_or_default();
    (*inner).pairs.retain(|(k, _)| k != &key);
    (*inner).pairs.push((key, val));
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn sp_sort(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    if !inner.is_null() {
        (*inner).pairs.sort_by(|a, b| a.0.cmp(&b.0));
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn sp_to_string(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    js_string(ctx, &build_search_string(&(*inner).pairs))
}

pub(super) unsafe extern "C" fn sp_length_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = sp_opaque(rt, this_val);
    let len = if inner.is_null() {
        0
    } else {
        (*inner).pairs.len()
    };
    sys::js_float64(len as f64)
}
