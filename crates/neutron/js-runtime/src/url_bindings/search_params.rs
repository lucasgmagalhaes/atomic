//! `URLSearchParams` class (core — iterator methods split into
//! `search_params_iterators.rs`).

use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::js_helpers::{define_getter as define_readonly, define_method};

use super::{get_property, read_js_string, set_property};

pub(crate) const URL_SEARCH_PARAMS_CLASS_KIND: &str = "URLSearchParams";

pub(super) struct UrlSearchParamsInner {
    pub(super) pairs: Vec<(String, String)>,
}

pub(super) unsafe fn sp_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut UrlSearchParamsInner {
    let class_id = crate::class_registry::class_id_for(rt, URL_SEARCH_PARAMS_CLASS_KIND);
    sys::JS_GetOpaque(this_val, class_id) as *mut UrlSearchParamsInner
}

pub(super) unsafe extern "C" fn sp_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = sp_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

unsafe fn parse_sp_init(ctx: *mut sys::JSContext, init: sys::JSValue) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    if init.tag == sys::JS_TAG_STRING {
        if let Some(s) = read_js_string(ctx, init) {
            if s.is_empty() {
                return pairs;
            }
            for part in s.split('&') {
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
                pairs.push((k, v));
            }
        }
        return pairs;
    }
    if sys::JS_IsArray(init) {
        let mut len: i64 = 0;
        sys::JS_GetLength(ctx, init, &mut len);
        for i in 0..(len as u32) {
            let pair = sys::JS_GetPropertyUint32(ctx, init, i);
            if sys::JS_IsArray(pair) {
                let mut plen: i64 = 0;
                sys::JS_GetLength(ctx, pair, &mut plen);
                if plen >= 2 {
                    let k = sys::JS_GetPropertyUint32(ctx, pair, 0);
                    let v = sys::JS_GetPropertyUint32(ctx, pair, 1);
                    let key = read_js_string(ctx, k).unwrap_or_default();
                    let val = read_js_string(ctx, v).unwrap_or_default();
                    sys::JS_FreeValue(ctx, k);
                    sys::JS_FreeValue(ctx, v);
                    pairs.push((key, val));
                }
            }
            sys::JS_FreeValue(ctx, pair);
        }
    }
    pairs
}

pub(super) unsafe fn build_search_string(pairs: &[(String, String)]) -> String {
    if pairs.is_empty() {
        return String::new();
    }
    let mut parts = Vec::with_capacity(pairs.len());
    for (k, v) in pairs {
        parts.push(format!(
            "{}={}",
            urlencoding::encode(k),
            urlencoding::encode(v)
        ));
    }
    parts.join("&")
}

/// Helper: get the `_sp` URLSearchParams object attached to a URL instance
/// and return a clone of its pairs. Caller must free `sp_val`.
pub(super) unsafe fn get_sp_pairs_for_url(
    ctx: *mut sys::JSContext,
    url_this: sys::JSValue,
) -> (sys::JSValue, Option<Vec<(String, String)>>) {
    let rt = sys::JS_GetRuntime(ctx);
    let sp_val = get_property(ctx, url_this, "_sp");
    if sp_val.tag == sys::JS_TAG_OBJECT {
        let sp_inner = sp_opaque(rt, sp_val);
        if !sp_inner.is_null() {
            return (sp_val, Some((*sp_inner).pairs.clone()));
        }
    }
    (sp_val, None)
}

/// Helper: overwrite the `_sp` URLSearchParams's pairs from a parsed query.
pub(super) unsafe fn set_sp_pairs_for_url(
    ctx: *mut sys::JSContext,
    url_this: sys::JSValue,
    pairs: Vec<(String, String)>,
) {
    let rt = sys::JS_GetRuntime(ctx);
    let sp_val = get_property(ctx, url_this, "_sp");
    if sp_val.tag == sys::JS_TAG_OBJECT {
        let sp_inner = sp_opaque(rt, sp_val);
        if !sp_inner.is_null() {
            (*sp_inner).pairs = pairs;
        }
    }
    sys::JS_FreeValue(ctx, sp_val);
}

unsafe extern "C" fn sp_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::ensure_class(
        rt,
        URL_SEARCH_PARAMS_CLASS_KIND,
        &sys::JSClassDef {
            class_name: b"URLSearchParams\0".as_ptr().cast(),
            finalizer: Some(sp_finalizer),
            gc_mark: std::ptr::null_mut(),
            call: std::ptr::null_mut(),
            exotic: std::ptr::null_mut(),
        },
    );
    let init = if argc >= 1 {
        *argv
    } else {
        sys::js_undefined()
    };
    let pairs = parse_sp_init(ctx, init);
    let inner = Box::new(UrlSearchParamsInner { pairs });
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    sys::JS_SetOpaque(obj, Box::into_raw(inner) as *mut std::os::raw::c_void);
    obj
}

/// Registers `URLSearchParams` class and creates a prototype with methods.
pub(super) unsafe fn register_url_search_params(ctx: *mut sys::JSContext) {
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::ensure_class(
        rt,
        URL_SEARCH_PARAMS_CLASS_KIND,
        &sys::JSClassDef {
            class_name: b"URLSearchParams\0".as_ptr().cast(),
            finalizer: Some(sp_finalizer),
            gc_mark: std::ptr::null_mut(),
            call: std::ptr::null_mut(),
            exotic: std::ptr::null_mut(),
        },
    );

    let proto = sys::JS_NewObject(ctx);
    define_method(
        ctx,
        proto,
        "append",
        super::search_params_methods::sp_append,
        2,
    );
    define_method(
        ctx,
        proto,
        "delete",
        super::search_params_methods::sp_delete,
        1,
    );
    define_method(ctx, proto, "get", super::search_params_methods::sp_get, 1);
    define_method(
        ctx,
        proto,
        "getAll",
        super::search_params_methods::sp_get_all,
        1,
    );
    define_method(ctx, proto, "has", super::search_params_methods::sp_has, 1);
    define_method(ctx, proto, "set", super::search_params_methods::sp_set, 2);
    define_method(ctx, proto, "sort", super::search_params_methods::sp_sort, 0);
    define_method(
        ctx,
        proto,
        "toString",
        super::search_params_methods::sp_to_string,
        0,
    );
    define_method(
        ctx,
        proto,
        "entries",
        super::search_params_iterators::sp_entries,
        0,
    );
    define_method(
        ctx,
        proto,
        "keys",
        super::search_params_iterators::sp_keys,
        0,
    );
    define_method(
        ctx,
        proto,
        "values",
        super::search_params_iterators::sp_values,
        0,
    );
    define_method(
        ctx,
        proto,
        "forEach",
        super::search_params_iterators::sp_for_each,
        1,
    );
    define_readonly(
        ctx,
        proto,
        "length",
        super::search_params_methods::sp_length_get,
    );
    sys::JS_SetClassProto(ctx, class_id, proto);

    let global = sys::JS_GetGlobalObject(ctx);
    let ctor = sys::JS_NewCFunction2(
        ctx,
        sp_constructor,
        b"URLSearchParams\0".as_ptr().cast(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    set_property(ctx, global, "URLSearchParams", ctor);
    sys::JS_FreeValue(ctx, global);
}
