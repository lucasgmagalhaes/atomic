//! `location` (bare global, `window.location` for free since `window` is
//! just an alias for the global object — see `crate::window` — and
//! `document.location`) — read-only properties derived live from
//! `HostState.url` (see `crate::host_state`), same degrade-to-empty
//! pattern `document.cookie`/`localStorage` already use when there's no
//! real backing state (`None` for a plain `Context::with_dom`/the built-in
//! demo page, which has no real navigated URL).
//!
//! No write side (`location.href = ...`, `assign()`, `replace()`,
//! `reload()`): those all trigger a real navigation in a real browser, and
//! this engine has no channel for JS to ask its host to do that yet — a
//! documented scope cut, not a silent partial implementation. Same-document
//! URL changes (`history.pushState`/`replaceState`) *do* update what
//! `location` reflects — see `crate::history`.
use std::ffi::CString;

use quickjs_sys as sys;

type Getter = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue) -> sys::JSValue;

unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
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
    for (name, getter) in [
        ("href", href_get as Getter),
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
