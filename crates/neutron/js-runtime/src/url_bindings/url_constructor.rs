//! `URL`'s constructor — split out of `url.rs` to keep it under this
//! project's per-file line convention. Still genuinely part of the
//! `URL` class, wired as the global `URL` constructor by
//! `url::register_url` via `super::url_constructor::url_constructor`.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::read_js_string;
use super::search_params::{sp_finalizer, UrlSearchParamsInner, URL_SEARCH_PARAMS_CLASS_KIND};
use super::type_error;
use super::url::{url_finalizer, UrlInner, URL_CLASS_KIND};

pub(super) unsafe extern "C" fn url_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return type_error(ctx, "URL: 1 argument required, but only 0 present.");
    }
    let url_str = match read_js_string(ctx, *argv) {
        Some(s) => s,
        None => return type_error(ctx, "URL: invalid URL"),
    };
    let parsed = if argc >= 2 {
        let base_str = read_js_string(ctx, *argv.add(1)).unwrap_or_default();
        match url::Url::parse(&base_str) {
            Ok(base) => match base.join(&url_str) {
                Ok(u) => u,
                Err(_) => return type_error(ctx, &format!("URL: invalid URL '{url_str}'")),
            },
            Err(_) => return type_error(ctx, &format!("URL: invalid base URL '{base_str}'")),
        }
    } else {
        match url::Url::parse(&url_str) {
            Ok(u) => u,
            Err(_) => return type_error(ctx, &format!("URL: invalid URL '{url_str}'")),
        }
    };

    let search_pairs: Vec<(String, String)> = parsed
        .query_pairs()
        .into_owned()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();

    let rt = sys::JS_GetRuntime(ctx);
    let url_class_id = crate::class_registry::ensure_class(
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

    let obj = sys::JS_NewObjectClass(ctx, url_class_id);
    let inner = Box::new(UrlInner { url: parsed });
    sys::JS_SetOpaque(obj, Box::into_raw(inner) as *mut std::os::raw::c_void);

    let sp_class_id = crate::class_registry::ensure_class(
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
    let sp_inner = Box::new(UrlSearchParamsInner {
        pairs: search_pairs,
    });
    let sp_obj = sys::JS_NewObjectClass(ctx, sp_class_id);
    sys::JS_SetOpaque(sp_obj, Box::into_raw(sp_inner) as *mut std::os::raw::c_void);

    let sp_name = CString::new("_sp").unwrap();
    sys::JS_SetPropertyStr(ctx, obj, sp_name.as_ptr(), sp_obj);

    obj
}
