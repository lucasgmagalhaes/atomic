//! `back`/`forward`/`go` — split out from `history.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::helpers::{
    apply_current_url, entries, entry_at, entry_field, index, read_number, read_string, set_prop,
};
use super::INDEX_PROP;

/// Moves `history`'s index by `delta`, clamped to a no-op when the result
/// would fall outside the real entry list (matches the real DOM: navigating
/// past either end of the stack does nothing, it doesn't clamp to an edge
/// and re-fire `popstate`). Dispatches a real `popstate` on the global
/// object with `.state` set from the entry landed on, whenever the index
/// actually moved.
unsafe fn go_by(ctx: *mut sys::JSContext, this: sys::JSValue, delta: i64) {
    let list = entries(ctx, this);
    let mut len = 0;
    sys::JS_GetLength(ctx, list, &mut len);
    sys::JS_FreeValue(ctx, list);
    let current = index(ctx, this);
    let target = current + delta;
    if target < 0 || target >= len || target == current {
        return;
    }
    set_prop(ctx, this, INDEX_PROP, sys::js_float64(target as f64));
    let entry = entry_at(ctx, this, target);
    let url_value = entry_field(ctx, entry, "url");
    let url_string = if url_value.tag == sys::JS_TAG_STRING {
        read_string(ctx, url_value)
    } else {
        None
    };
    sys::JS_FreeValue(ctx, url_value);
    apply_current_url(ctx, url_string.as_deref());

    let state_value = entry_field(ctx, entry, "state");
    sys::JS_FreeValue(ctx, entry);
    let event = crate::events::create_event(ctx, "popstate", false, false);
    if !sys::js_is_exception(&event) {
        let state_name = std::ffi::CString::new("state").unwrap();
        sys::JS_SetPropertyStr(ctx, event, state_name.as_ptr(), state_value);
        let global = sys::JS_GetGlobalObject(ctx);
        crate::events::dispatch_event_object(ctx, global, event);
        sys::JS_FreeValue(ctx, global);
    } else {
        sys::JS_FreeValue(ctx, state_value);
    }
}

pub(super) unsafe extern "C" fn back(
    ctx: *mut sys::JSContext,
    this: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    go_by(ctx, this, -1);
    sys::js_undefined()
}
pub(super) unsafe extern "C" fn forward(
    ctx: *mut sys::JSContext,
    this: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    go_by(ctx, this, 1);
    sys::js_undefined()
}
pub(super) unsafe extern "C" fn go(
    ctx: *mut sys::JSContext,
    this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let Some(delta) = read_number(*argv) else {
        return sys::js_undefined();
    };
    go_by(ctx, this, delta as i64);
    sys::js_undefined()
}
