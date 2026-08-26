//! `document.cookie`: a real accessor property backed by
//! `storage::cookies::CookieJar` (see `Context::with_storage`) — reading
//! it returns the real `"name=value; name2=value2"` header-shaped string
//! for the current host, writing it parses the assignment the same way a
//! real `Set-Cookie` header would (`document.cookie = "a=1; Path=/;
//! Max-Age=3600"` is legal in real browsers for exactly this reason: the
//! grammar's shared between the JS setter and the HTTP header).
//!
//! Simplifications: there's no per-document URL/navigation concept in
//! this crate yet, so "current path" is always `/` and "secure context"
//! is always treated as true (a `Secure`-flagged cookie is never silently
//! dropped here, unlike a real `http://` page) — both are documented gaps
//! shared with the rest of `js-runtime` (see `performance.now`'s doc on
//! the same missing navigation concept). Without `Context::with_storage`
//! configuring a jar, the getter always returns `""` and the setter is a
//! no-op — not an error, matching how a lot of this crate's bindings
//! degrade gracefully rather than requiring every `Context` to be fully
//! wired.
use std::ffi::CString;

use quickjs_sys as sys;

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
    let mut len: usize = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, val, false);
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
    let s = String::from_utf8_lossy(bytes).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe extern "C" fn cookie_get(ctx: *mut sys::JSContext, _this_val: sys::JSValue) -> sys::JSValue {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return new_js_string(ctx, "");
    }
    let Some(jar) = (*state).cookies.as_ref() else {
        return new_js_string(ctx, "");
    };
    let host = &(*state).host;
    let header = jar.header_value(host, "/", true).unwrap_or_default();
    new_js_string(ctx, &header)
}

unsafe extern "C" fn cookie_set(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return sys::js_undefined();
    }
    let Some(assignment) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    let host = (*state).host.clone();
    if let Some(jar) = (*state).cookies.as_mut() {
        let _ = jar.set_from_header(&assignment, &host);
    }
    sys::js_undefined()
}

/// Defines the `cookie` accessor on `document` (fetched via
/// `crate::document::get_or_create`, the same shared-`document`-object
/// mechanism `dom_bindings`/`page_visibility` use, so this doesn't matter
/// which of those ran first or whether either did at all).
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let document = crate::document::get_or_create(ctx);
    let name = CString::new("cookie").unwrap();

    type Getter =
        unsafe extern "C" fn(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue;
    type Setter = unsafe extern "C" fn(
        ctx: *mut sys::JSContext,
        this_val: sys::JSValue,
        val: sys::JSValue,
    ) -> sys::JSValue;

    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(cookie_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let setter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(cookie_set),
        name.as_ptr(),
        1,
        sys::JS_CFUNC_SETTER,
        0,
    );

    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        document,
        atom,
        getter,
        setter,
        sys::JS_PROP_HAS_GET
            | sys::JS_PROP_HAS_SET
            | sys::JS_PROP_CONFIGURABLE
            | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
    sys::JS_FreeValue(ctx, document);
}
