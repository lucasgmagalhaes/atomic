//! Low-level property/read helpers shared by every `history` handler —
//! split out from `history.rs`.

use std::ffi::CString;

use quickjs_sys as sys;

use super::{ENTRIES_PROP, INDEX_PROP};

pub(super) unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}
pub(super) unsafe fn throw_type_error(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_string(ctx, s))
}
pub(super) unsafe fn read_string(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<String> {
    let mut len = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, value, false);
    if ptr.is_null() {
        return None;
    }
    let s = String::from_utf8_lossy(std::slice::from_raw_parts(ptr as *const u8, len)).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}
/// Reads a number already tagged `INT`/`FLOAT64` — every number this module
/// itself reads back was written by itself via `js_float64`/`argv` numeric
/// literals, so no generic `ToNumber` coercion is needed (same scope cut
/// `event_subclasses.rs::read_number` documents).
pub(super) unsafe fn read_number(val: sys::JSValue) -> Option<f64> {
    match val.tag {
        sys::JS_TAG_INT => Some(val.u.int32 as f64),
        sys::JS_TAG_FLOAT64 => Some(val.u.float64),
        _ => None,
    }
}
pub(super) unsafe fn get_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &[u8],
) -> sys::JSValue {
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr() as *const _)
}
pub(super) unsafe fn set_prop(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &[u8],
    value: sys::JSValue,
) {
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr() as *const _, value);
}

pub(super) unsafe fn entries(ctx: *mut sys::JSContext, history: sys::JSValue) -> sys::JSValue {
    get_prop(ctx, history, ENTRIES_PROP)
}
pub(super) unsafe fn index(ctx: *mut sys::JSContext, history: sys::JSValue) -> i64 {
    let value = get_prop(ctx, history, INDEX_PROP);
    let i = read_number(value).unwrap_or(0.0) as i64;
    sys::JS_FreeValue(ctx, value);
    i
}
pub(super) unsafe fn entry_at(
    ctx: *mut sys::JSContext,
    history: sys::JSValue,
    at: i64,
) -> sys::JSValue {
    let list = entries(ctx, history);
    let entry = sys::JS_GetPropertyUint32(ctx, list, at as u32);
    sys::JS_FreeValue(ctx, list);
    entry
}
pub(super) unsafe fn entry_field(
    ctx: *mut sys::JSContext,
    entry: sys::JSValue,
    name: &str,
) -> sys::JSValue {
    let cname = CString::new(name).unwrap();
    sys::JS_GetPropertyStr(ctx, entry, cname.as_ptr())
}

pub(super) unsafe fn make_entry(
    ctx: *mut sys::JSContext,
    state: sys::JSValue,
    url: sys::JSValue,
    title: &str,
) -> sys::JSValue {
    let entry = sys::JS_NewObject(ctx);
    let state_name = CString::new("state").unwrap();
    sys::JS_SetPropertyStr(ctx, entry, state_name.as_ptr(), state);
    let url_name = CString::new("url").unwrap();
    sys::JS_SetPropertyStr(ctx, entry, url_name.as_ptr(), url);
    let title_name = CString::new("title").unwrap();
    sys::JS_SetPropertyStr(ctx, entry, title_name.as_ptr(), new_string(ctx, title));
    entry
}

/// Resolves `provided` (a `pushState`/`replaceState` url argument) against
/// the current `location`, falling back to treating it as already-absolute,
/// and finally to the raw string if it isn't real URL syntax at all (best
/// effort — matches this crate's general "don't hard-fail on a
/// non-URL-shaped string" leniency elsewhere). `None` keeps whatever
/// `location` currently reflects (a `pushState(state, title)` call with no
/// third argument).
pub(super) unsafe fn resolve_url(
    ctx: *mut sys::JSContext,
    provided: Option<String>,
) -> Option<String> {
    let current = crate::location::current_url(ctx);
    match provided {
        None => current.map(|u| u.to_string()),
        Some(raw) => current
            .as_ref()
            .and_then(|base| base.join(&raw).ok())
            .or_else(|| url::Url::parse(&raw).ok())
            .map(|u| u.to_string())
            .or(Some(raw)),
    }
}

pub(super) unsafe fn apply_current_url(ctx: *mut sys::JSContext, url: Option<&str>) {
    if let Some(url) = url {
        let state = crate::host_state::get(ctx);
        if !state.is_null() {
            (*state).url = Some(url.to_string());
        }
    }
}

pub(super) unsafe fn read_state_args(
    ctx: *mut sys::JSContext,
    argc: std::os::raw::c_int,
    argv: *mut sys::JSValue,
) -> (sys::JSValue, Option<String>) {
    let state_arg = if argc >= 1 {
        crate::value_bridge::deep_clone(ctx, *argv)
    } else {
        sys::js_null()
    };
    let url_arg = if argc >= 3 {
        read_string(ctx, *argv.add(2))
    } else {
        None
    };
    (state_arg, url_arg)
}
