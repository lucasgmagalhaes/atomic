//! `pushState`/`replaceState` — split out from `history.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::helpers::{
    apply_current_url, entries, index, make_entry, new_string, read_state_args, resolve_url,
    set_prop,
};
use super::{ENTRIES_PROP, INDEX_PROP, MAX_ENTRIES};

pub(super) unsafe extern "C" fn push_state(
    ctx: *mut sys::JSContext,
    this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let list = entries(ctx, this);
    let mut len = 0;
    sys::JS_GetLength(ctx, list, &mut len);
    if len >= MAX_ENTRIES {
        sys::JS_FreeValue(ctx, list);
        return super::helpers::throw_type_error(ctx, "history entry limit exceeded");
    }
    let current = index(ctx, this);
    let (state_arg, url_arg) = read_state_args(ctx, argc, argv);
    let resolved = resolve_url(ctx, url_arg);
    let url_value = resolved
        .as_deref()
        .map(|u| new_string(ctx, u))
        .unwrap_or_else(sys::js_null);
    let entry = make_entry(ctx, state_arg, url_value, "");

    let new_list = sys::JS_NewArray(ctx);
    for i in 0..=current {
        sys::JS_SetPropertyUint32(
            ctx,
            new_list,
            i as u32,
            sys::JS_GetPropertyUint32(ctx, list, i as u32),
        );
    }
    sys::JS_FreeValue(ctx, list);
    let new_index = current + 1;
    sys::JS_SetPropertyUint32(ctx, new_list, new_index as u32, entry);
    set_prop(ctx, this, ENTRIES_PROP, new_list);
    set_prop(ctx, this, INDEX_PROP, sys::js_float64(new_index as f64));
    apply_current_url(ctx, resolved.as_deref());
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn replace_state(
    ctx: *mut sys::JSContext,
    this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let current = index(ctx, this);
    let (state_arg, url_arg) = read_state_args(ctx, argc, argv);
    let resolved = resolve_url(ctx, url_arg);
    let url_value = resolved
        .as_deref()
        .map(|u| new_string(ctx, u))
        .unwrap_or_else(sys::js_null);
    let entry = make_entry(ctx, state_arg, url_value, "");

    let list = entries(ctx, this);
    sys::JS_SetPropertyUint32(ctx, list, current as u32, entry);
    sys::JS_FreeValue(ctx, list);
    apply_current_url(ctx, resolved.as_deref());
    sys::js_undefined()
}
