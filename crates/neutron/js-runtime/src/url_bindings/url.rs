//! `URL` class (core — `host`/`hostname`/`port`/`pathname`/`search`/
//! `hash`/`toString`/`toJSON` split into `url_parts.rs`).

use quickjs_sys as sys;

use crate::js_helpers::{
    define_getter as define_readonly, define_getter_setter as define_readwrite, define_method,
};

use super::search_params::{build_search_string, get_sp_pairs_for_url, set_sp_pairs_for_url};
use super::{get_property, js_string, read_js_string, set_property};

pub(crate) const URL_CLASS_KIND: &str = "URL";

pub(super) struct UrlInner {
    pub(super) url: url::Url,
}

pub(super) unsafe fn url_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut UrlInner {
    let class_id = crate::class_registry::class_id_for(rt, URL_CLASS_KIND);
    sys::JS_GetOpaque(this_val, class_id) as *mut UrlInner
}

pub(super) unsafe extern "C" fn url_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = url_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

pub(super) unsafe fn url_display_from_sp(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    let (sp_val, pairs_opt) = get_sp_pairs_for_url(ctx, this_val);
    let pairs_ref = pairs_opt.as_ref();
    let mut url = (*inner).url.clone();
    if let Some(pairs) = pairs_ref {
        let search = build_search_string(pairs);
        if search.is_empty() {
            url.set_query(None);
        } else {
            url.set_query(Some(&search));
        }
    }
    sys::JS_FreeValue(ctx, sp_val);
    js_string(ctx, &url.to_string())
}

// -- URL JS getters/setters --

pub(super) unsafe extern "C" fn url_href_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    url_display_from_sp(ctx, this_val)
}

unsafe extern "C" fn url_href_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return sys::js_undefined();
    }
    if let Some(s) = read_js_string(ctx, val) {
        if let Ok(new_url) = url::Url::parse(&s) {
            let new_pairs: Vec<(String, String)> = new_url
                .query_pairs()
                .into_owned()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect();
            set_sp_pairs_for_url(ctx, this_val, new_pairs);
            (*inner).url = new_url;
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn url_origin_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "null");
    }
    js_string(ctx, &(*inner).url.origin().ascii_serialization())
}

unsafe extern "C" fn url_protocol_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    js_string(ctx, &format!("{}:", (*inner).url.scheme()))
}

unsafe extern "C" fn url_protocol_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if !inner.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            let scheme = s.trim_end_matches(':');
            let _ = (*inner).url.set_scheme(scheme);
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn url_username_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    js_string(ctx, (*inner).url.username())
}

unsafe extern "C" fn url_username_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if !inner.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            let _ = (*inner).url.set_username(&s);
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn url_password_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    js_string(ctx, (*inner).url.password().unwrap_or(""))
}

unsafe extern "C" fn url_password_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if !inner.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            let _ = (*inner).url.set_password(Some(&s));
        }
    }
    sys::js_undefined()
}

/// Registers `URL` class with a prototype containing getters/setters.
pub(super) unsafe fn register_url(ctx: *mut sys::JSContext) {
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::ensure_class(
        rt,
        URL_CLASS_KIND,
        &sys::JSClassDef {
            class_name: b"URL\0".as_ptr().cast(),
            finalizer: Some(url_finalizer),
            gc_mark: std::ptr::null_mut(),
            call: std::ptr::null_mut(),
            exotic: std::ptr::null_mut(),
        },
    );

    let proto = sys::JS_NewObject(ctx);
    define_readwrite(ctx, proto, "href", url_href_get, url_href_set);
    define_readonly(ctx, proto, "origin", url_origin_get);
    define_readwrite(ctx, proto, "protocol", url_protocol_get, url_protocol_set);
    define_readwrite(ctx, proto, "username", url_username_get, url_username_set);
    define_readwrite(ctx, proto, "password", url_password_get, url_password_set);
    define_readwrite(
        ctx,
        proto,
        "host",
        super::url_parts::url_host_get,
        super::url_parts::url_host_set,
    );
    define_readwrite(
        ctx,
        proto,
        "hostname",
        super::url_parts::url_hostname_get,
        super::url_parts::url_hostname_set,
    );
    define_readwrite(
        ctx,
        proto,
        "port",
        super::url_parts::url_port_get,
        super::url_parts::url_port_set,
    );
    define_readwrite(
        ctx,
        proto,
        "pathname",
        super::url_parts::url_pathname_get,
        super::url_parts::url_pathname_set,
    );
    define_readwrite(
        ctx,
        proto,
        "search",
        super::url_parts::url_search_get,
        super::url_parts::url_search_set,
    );
    define_readonly(
        ctx,
        proto,
        "searchParams",
        super::url_parts::url_search_params_get,
    );
    define_readwrite(
        ctx,
        proto,
        "hash",
        super::url_parts::url_hash_get,
        super::url_parts::url_hash_set,
    );
    define_method(
        ctx,
        proto,
        "toString",
        super::url_parts::url_to_string_fn,
        0,
    );
    define_method(ctx, proto, "toJSON", super::url_parts::url_to_json, 0);
    sys::JS_SetClassProto(ctx, class_id, proto);

    let global = sys::JS_GetGlobalObject(ctx);
    let ctor = sys::JS_NewCFunction2(
        ctx,
        super::url_constructor::url_constructor,
        b"URL\0".as_ptr().cast(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );

    let existing_url = get_property(ctx, global, "URL");
    if existing_url.tag == sys::JS_TAG_OBJECT {
        let create_object_url = get_property(ctx, existing_url, "createObjectURL");
        if create_object_url.tag != sys::JS_TAG_UNDEFINED {
            set_property(ctx, ctor, "createObjectURL", create_object_url);
        }
        let revoke_object_url = get_property(ctx, existing_url, "revokeObjectURL");
        if revoke_object_url.tag != sys::JS_TAG_UNDEFINED {
            set_property(ctx, ctor, "revokeObjectURL", revoke_object_url);
        }
        sys::JS_FreeValue(ctx, existing_url);
    } else {
        sys::JS_FreeValue(ctx, existing_url);
    }

    set_property(ctx, global, "URL", ctor);
    sys::JS_FreeValue(ctx, global);
}
