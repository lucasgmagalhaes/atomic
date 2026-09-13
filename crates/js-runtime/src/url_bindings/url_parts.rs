//! `URL`'s remaining getters/setters/methods (`host`/`hostname`/`port`/
//! `pathname`/`search`/`hash`/`searchParams`/`toString`/`toJSON`) — split
//! out of `url.rs` to keep it under this project's per-file line
//! convention. Still genuinely part of the `URL` class, registered onto
//! its prototype by `url::register_url` via `super::url_parts::...`
//! paths.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::search_params::{build_search_string, get_sp_pairs_for_url, set_sp_pairs_for_url};
use super::url::{url_href_get, url_opaque};
use super::{get_property, js_string, read_js_string};

pub(super) unsafe extern "C" fn url_host_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    match ((*inner).url.host_str(), (*inner).url.port()) {
        (Some(host), Some(port)) => js_string(ctx, &format!("{host}:{port}")),
        (Some(host), None) => js_string(ctx, host),
        (None, _) => js_string(ctx, ""),
    }
}

pub(super) unsafe extern "C" fn url_host_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if !inner.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            let _ = (*inner).url.set_host(Some(&s));
        }
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn url_hostname_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    js_string(ctx, (*inner).url.host_str().unwrap_or(""))
}

pub(super) unsafe extern "C" fn url_hostname_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if !inner.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            let _ = (*inner).url.set_host(Some(&s));
        }
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn url_port_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    js_string(
        ctx,
        &(*inner).url.port().map_or(String::new(), |p| p.to_string()),
    )
}

pub(super) unsafe extern "C" fn url_port_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if !inner.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            if s.is_empty() {
                let _ = (*inner).url.set_port(None);
            } else if let Ok(port) = s.parse::<u16>() {
                let _ = (*inner).url.set_port(Some(port));
            }
        }
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn url_pathname_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    js_string(ctx, (*inner).url.path())
}

pub(super) unsafe extern "C" fn url_pathname_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if !inner.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            let _ = (*inner).url.set_path(&s);
        }
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn url_search_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    let (sp_val, pairs_opt) = get_sp_pairs_for_url(ctx, this_val);
    if let Some(pairs) = &pairs_opt {
        let search = build_search_string(pairs);
        sys::JS_FreeValue(ctx, sp_val);
        if search.is_empty() {
            return js_string(ctx, "");
        }
        return js_string(ctx, &format!("?{search}"));
    }
    sys::JS_FreeValue(ctx, sp_val);
    js_string(ctx, "")
}

pub(super) unsafe extern "C" fn url_search_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let new_pairs = if let Some(s) = read_js_string(ctx, val) {
        let s = s.trim_start_matches('?');
        if s.is_empty() {
            Vec::new()
        } else {
            s.split('&')
                .filter_map(|part| {
                    let (k, v) = match part.find('=') {
                        Some(pos) => (
                            urlencoding::decode(&part[..pos])
                                .unwrap_or_default()
                                .into_owned(),
                            urlencoding::decode(&part[pos + 1..])
                                .unwrap_or_default()
                                .into_owned(),
                        ),
                        None => (
                            urlencoding::decode(part).unwrap_or_default().into_owned(),
                            String::new(),
                        ),
                    };
                    if k.is_empty() {
                        None
                    } else {
                        Some((k, v))
                    }
                })
                .collect()
        }
    } else {
        Vec::new()
    };
    set_sp_pairs_for_url(ctx, this_val, new_pairs);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn url_search_params_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_property(ctx, this_val, "_sp")
}

pub(super) unsafe extern "C" fn url_hash_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if inner.is_null() {
        return js_string(ctx, "");
    }
    match (*inner).url.fragment() {
        Some(f) => js_string(ctx, &format!("#{f}")),
        None => js_string(ctx, ""),
    }
}

pub(super) unsafe extern "C" fn url_hash_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let inner = url_opaque(rt, this_val);
    if !inner.is_null() {
        if let Some(s) = read_js_string(ctx, val) {
            let s = s.trim_start_matches('#');
            if s.is_empty() {
                (*inner).url.set_fragment(None);
            } else {
                (*inner).url.set_fragment(Some(s));
            }
        }
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn url_to_string_fn(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    url_href_get(ctx, this_val)
}

pub(super) unsafe extern "C" fn url_to_json(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    url_to_string_fn(ctx, this_val, argc, argv)
}
