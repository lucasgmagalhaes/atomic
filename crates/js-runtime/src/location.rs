//! `location` (bare global, `window.location` for free since `window` is
//! just an alias for the global object — see `crate::window` — and
//! `document.location`) — read-only properties derived live from
//! `HostState.url` (see `crate::host_state`), same degrade-to-empty
//! pattern `document.cookie`/`localStorage` already use when there's no
//! real backing state (`None` for a plain `Context::with_dom`/the built-in
//! demo page, which has no real navigated URL).
//!
//! Real write side: `location.href = ...`, `assign(url)`, `replace(url)`,
//! `reload()` all write into `HostState.pending_navigation` — the same
//! real "JS requests navigation, host performs it" channel a real `<a
//! href>` click's default action already uses
//! (`dom_bindings::forms::run_default_click_action`), consumed by
//! `profile-worker`'s own `navigate_if_requested` after any command that
//! could run a click handler. `assign`/`replace` request the identical
//! real navigation here — no distinct "replace this history entry" vs
//! "push a new one" treatment, unlike same-document
//! `history.pushState`/`replaceState` (`crate::history`), which is a
//! genuinely different, same-document-only mechanism. `reload()`
//! re-requests the current URL. Same real limitation every other
//! `pending_navigation` writer has: only consumed after a command that
//! calls `navigate_if_requested` — setting `location.href` from a bare
//! timer callback with no follow-up command won't navigate until the
//! next one runs.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::js_helpers::{define_getter_setter, define_method, Getter};

unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}

/// No string/object coercion — same scope cut every other ad-hoc
/// `read_string` in this crate documents (no shared one exists; see
/// `js_helpers`'s own module doc on why `dom_bindings`'s tree is
/// separate).
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

/// Writes `href` into `HostState.pending_navigation` - the real channel
/// `profile-worker`'s `navigate_if_requested` consumes, same one a real
/// `<a href>` click's default action already uses. No-op (silently, same
/// as a real browser's location write in a context with nothing to
/// navigate) if there's no real `HostState` at all.
unsafe fn request_navigation(ctx: *mut sys::JSContext, href: String) {
    let state = crate::host_state::get(ctx);
    if !state.is_null() {
        (*state).pending_navigation = Some(href);
    }
}

/// The page's current URL, or `None` if unset/unparseable — every getter
/// below degrades to `""` in that case rather than panicking.
pub(crate) unsafe fn current_url(ctx: *mut sys::JSContext) -> Option<url::Url> {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return None;
    }
    (*state)
        .url
        .as_ref()
        .and_then(|raw| url::Url::parse(raw).ok())
}

unsafe fn or_empty(ctx: *mut sys::JSContext, value: Option<String>) -> sys::JSValue {
    new_string(ctx, value.as_deref().unwrap_or(""))
}

unsafe extern "C" fn href_get(ctx: *mut sys::JSContext, _this: sys::JSValue) -> sys::JSValue {
    or_empty(ctx, current_url(ctx).map(|u| u.to_string()))
}
unsafe extern "C" fn href_set(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    if let Some(href) = read_string(ctx, val) {
        request_navigation(ctx, href);
    }
    sys::js_undefined()
}
unsafe extern "C" fn assign(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc >= 1 {
        if let Some(href) = read_string(ctx, *argv) {
            request_navigation(ctx, href);
        }
    }
    sys::js_undefined()
}
unsafe extern "C" fn replace(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    // Real spec distinguishes `replace` from `assign` by history-entry
    // treatment - see this module's own doc on why that split isn't
    // modeled here (a cross-document navigation via `pending_navigation`
    // doesn't interact with the same-document `history` stack at all).
    assign(ctx, _this, argc, argv)
}
unsafe extern "C" fn reload(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    if let Some(href) = current_url(ctx).map(|u| u.to_string()) {
        request_navigation(ctx, href);
    }
    sys::js_undefined()
}
unsafe extern "C" fn protocol_get(ctx: *mut sys::JSContext, _this: sys::JSValue) -> sys::JSValue {
    or_empty(ctx, current_url(ctx).map(|u| format!("{}:", u.scheme())))
}
unsafe extern "C" fn host_get(ctx: *mut sys::JSContext, _this: sys::JSValue) -> sys::JSValue {
    or_empty(
        ctx,
        current_url(ctx).map(|u| match (u.host_str(), u.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            (None, _) => String::new(),
        }),
    )
}
unsafe extern "C" fn hostname_get(ctx: *mut sys::JSContext, _this: sys::JSValue) -> sys::JSValue {
    or_empty(
        ctx,
        current_url(ctx).and_then(|u| u.host_str().map(str::to_string)),
    )
}
unsafe extern "C" fn port_get(ctx: *mut sys::JSContext, _this: sys::JSValue) -> sys::JSValue {
    or_empty(
        ctx,
        current_url(ctx)
            .and_then(|u| u.port())
            .map(|p| p.to_string()),
    )
}
unsafe extern "C" fn pathname_get(ctx: *mut sys::JSContext, _this: sys::JSValue) -> sys::JSValue {
    or_empty(ctx, current_url(ctx).map(|u| u.path().to_string()))
}
unsafe extern "C" fn search_get(ctx: *mut sys::JSContext, _this: sys::JSValue) -> sys::JSValue {
    or_empty(
        ctx,
        current_url(ctx).map(|u| u.query().map(|q| format!("?{q}")).unwrap_or_default()),
    )
}
unsafe extern "C" fn hash_get(ctx: *mut sys::JSContext, _this: sys::JSValue) -> sys::JSValue {
    or_empty(
        ctx,
        current_url(ctx).map(|u| u.fragment().map(|f| format!("#{f}")).unwrap_or_default()),
    )
}
unsafe extern "C" fn origin_get(ctx: *mut sys::JSContext, _this: sys::JSValue) -> sys::JSValue {
    or_empty(
        ctx,
        current_url(ctx).map(|u| u.origin().ascii_serialization()),
    )
}

unsafe extern "C" fn to_string(
    ctx: *mut sys::JSContext,
    this: sys::JSValue,
    _argc: std::os::raw::c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    href_get(ctx, this)
}

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
    let location = sys::JS_NewObject(ctx);
    define_getter_setter(ctx, location, "href", href_get, href_set);
    for (name, getter) in [
        ("protocol", protocol_get as Getter),
        ("host", host_get as Getter),
        ("hostname", hostname_get as Getter),
        ("port", port_get as Getter),
        ("pathname", pathname_get as Getter),
        ("search", search_get as Getter),
        ("hash", hash_get as Getter),
        ("origin", origin_get as Getter),
    ] {
        define_readonly(ctx, location, name, getter);
    }
    let method_name = CString::new("toString").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        location,
        method_name.as_ptr(),
        sys::JS_NewCFunction2(
            ctx,
            to_string,
            method_name.as_ptr(),
            0,
            sys::JS_CFUNC_GENERIC,
            0,
        ),
    );
    define_method(ctx, location, "assign", assign, 1);
    define_method(ctx, location, "replace", replace, 1);
    define_method(ctx, location, "reload", reload, 0);

    let global = sys::JS_GetGlobalObject(ctx);
    let location_name = CString::new("location").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        global,
        location_name.as_ptr(),
        sys::JS_DupValue(ctx, location),
    );
    sys::JS_FreeValue(ctx, global);

    let document = crate::document::get_or_create(ctx);
    sys::JS_SetPropertyStr(ctx, document, location_name.as_ptr(), location);
    sys::JS_FreeValue(ctx, document);
}
