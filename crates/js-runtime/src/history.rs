//! `history` — real same-document navigation state (`pushState`/
//! `replaceState`/`back`/`forward`/`go`/`length`/`state`), backed by a
//! plain JS array kept as a hidden `__entries` own property on the
//! `history` object itself (each entry `{state, url, title}`), plus a
//! `__index` pointer into it — same "store native-ish state as plain JS
//! values on the object" convention `events.rs`'s listener records use,
//! rather than a separate native side table.
//!
//! Real deviation from spec: `state` is kept by direct reference (a duped
//! `JSValue`), not structured-cloned — mutating an object after
//! `pushState(obj, ...)` is visible through `history.state`/a later
//! `popstate` too, unlike a real browser's independent snapshot. Every
//! other JS-facing structured-clone-shaped API in this crate
//! (`postMessage`-less `workers`, `structuredClone`) has the same kind of
//! scope cut where a real deep clone would need more plumbing than the
//! feature is worth yet.
//!
//! `back`/`forward`/`go` only ever move within *this context's own*
//! `pushState`/`replaceState` stack — real browser history that predates
//! the current page load (whatever the host navigated through before this
//! script ran) isn't modeled, since nothing hands this crate that list.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

const ENTRIES_PROP: &[u8] = b"__entries\0";
const INDEX_PROP: &[u8] = b"__index\0";
const MAX_ENTRIES: i64 = 1000;

unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}
unsafe fn throw_type_error(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_string(ctx, s))
}
unsafe fn read_string(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<String> {
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
unsafe fn read_number(val: sys::JSValue) -> Option<f64> {
    match val.tag {
        sys::JS_TAG_INT => Some(val.u.int32 as f64),
        sys::JS_TAG_FLOAT64 => Some(val.u.float64),
        _ => None,
    }
}
unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &[u8]) -> sys::JSValue {
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr() as *const _)
}
unsafe fn set_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &[u8], value: sys::JSValue) {
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr() as *const _, value);
}

unsafe fn entries(ctx: *mut sys::JSContext, history: sys::JSValue) -> sys::JSValue {
    get_prop(ctx, history, ENTRIES_PROP)
}
unsafe fn index(ctx: *mut sys::JSContext, history: sys::JSValue) -> i64 {
    let value = get_prop(ctx, history, INDEX_PROP);
    let i = read_number(value).unwrap_or(0.0) as i64;
    sys::JS_FreeValue(ctx, value);
    i
}
unsafe fn entry_at(ctx: *mut sys::JSContext, history: sys::JSValue, at: i64) -> sys::JSValue {
    let list = entries(ctx, history);
    let entry = sys::JS_GetPropertyUint32(ctx, list, at as u32);
    sys::JS_FreeValue(ctx, list);
    entry
}
unsafe fn entry_field(ctx: *mut sys::JSContext, entry: sys::JSValue, name: &str) -> sys::JSValue {
    let cname = CString::new(name).unwrap();
    sys::JS_GetPropertyStr(ctx, entry, cname.as_ptr())
}

unsafe fn make_entry(
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
unsafe fn resolve_url(ctx: *mut sys::JSContext, provided: Option<String>) -> Option<String> {
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

unsafe fn apply_current_url(ctx: *mut sys::JSContext, url: Option<&str>) {
    if let Some(url) = url {
        let state = crate::host_state::get(ctx);
        if !state.is_null() {
            (*state).url = Some(url.to_string());
        }
    }
}

unsafe fn read_state_args(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> (sys::JSValue, Option<String>) {
    let state_arg = if argc >= 1 {
        sys::JS_DupValue(ctx, *argv)
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

unsafe extern "C" fn push_state(
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
        return throw_type_error(ctx, "history entry limit exceeded");
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

unsafe extern "C" fn replace_state(
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
        let state_name = CString::new("state").unwrap();
        sys::JS_SetPropertyStr(ctx, event, state_name.as_ptr(), state_value);
        let global = sys::JS_GetGlobalObject(ctx);
        crate::events::dispatch_event_object(ctx, global, event);
        sys::JS_FreeValue(ctx, global);
    } else {
        sys::JS_FreeValue(ctx, state_value);
    }
}

unsafe extern "C" fn back(
    ctx: *mut sys::JSContext,
    this: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    go_by(ctx, this, -1);
    sys::js_undefined()
}
unsafe extern "C" fn forward(
    ctx: *mut sys::JSContext,
    this: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    go_by(ctx, this, 1);
    sys::js_undefined()
}
unsafe extern "C" fn go(
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

unsafe extern "C" fn length_get(ctx: *mut sys::JSContext, this: sys::JSValue) -> sys::JSValue {
    let list = entries(ctx, this);
    let mut len = 0;
    sys::JS_GetLength(ctx, list, &mut len);
    sys::JS_FreeValue(ctx, list);
    sys::js_float64(len as f64)
}
unsafe extern "C" fn state_get(ctx: *mut sys::JSContext, this: sys::JSValue) -> sys::JSValue {
    let current = index(ctx, this);
    let entry = entry_at(ctx, this, current);
    let state = entry_field(ctx, entry, "state");
    sys::JS_FreeValue(ctx, entry);
    state
}

type Getter = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue) -> sys::JSValue;
unsafe fn define_readonly(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &str, getter: Getter) {
    let cname = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(getter),
        cname.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        obj,
        atom,
        f,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let history = sys::JS_NewObject(ctx);

    let initial_entries = sys::JS_NewArray(ctx);
    let initial_entry = make_entry(ctx, sys::js_null(), sys::js_null(), "");
    sys::JS_SetPropertyUint32(ctx, initial_entries, 0, initial_entry);
    set_prop(ctx, history, ENTRIES_PROP, initial_entries);
    set_prop(ctx, history, INDEX_PROP, sys::js_float64(0.0));

    define_readonly(ctx, history, "length", length_get as Getter);
    define_readonly(ctx, history, "state", state_get as Getter);

    for (name, func, arity) in [
        ("pushState", push_state as sys::JSCFunction, 3),
        ("replaceState", replace_state as sys::JSCFunction, 3),
        ("back", back as sys::JSCFunction, 0),
        ("forward", forward as sys::JSCFunction, 0),
        ("go", go as sys::JSCFunction, 1),
    ] {
        let cname = CString::new(name).unwrap();
        sys::JS_SetPropertyStr(
            ctx,
            history,
            cname.as_ptr(),
            sys::JS_NewCFunction2(ctx, func, cname.as_ptr(), arity, sys::JS_CFUNC_GENERIC, 0),
        );
    }

    let global = sys::JS_GetGlobalObject(ctx);
    let name = CString::new("history").unwrap();
    sys::JS_SetPropertyStr(ctx, global, name.as_ptr(), history);
    sys::JS_FreeValue(ctx, global);
}
