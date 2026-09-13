//! `XMLHttpRequest` — split out from `fetch_async/mod.rs`.

use std::ffi::CString;
use std::os::raw::c_int;
use std::sync::mpsc;
use std::thread;

use quickjs_sys as sys;

use super::helpers::{get_prop, new_js_string, read_js_string, set_num, set_str, throw_type_error};
use super::types::PendingXhr;
use super::with_state;

pub(super) unsafe extern "C" fn xhr_constructor(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    let proto = get_prop(ctx, new_target, "prototype");
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    // `JS_SetPrototype` takes `proto` by (const) reference, not
    // ownership - unlike `JS_SetPropertyStr` and friends, it doesn't
    // consume the value, so this call is unconditional either way.
    sys::JS_FreeValue(ctx, proto);
    set_num(ctx, obj, "readyState", 0.0);
    set_num(ctx, obj, "status", 0.0);
    set_str(ctx, obj, "responseText", "");
    obj
}

pub(super) unsafe extern "C" fn xhr_open(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc >= 2 {
        if let Some(method) = read_js_string(ctx, *argv) {
            let upper = method.to_ascii_uppercase();
            set_str(ctx, this_val, "_method", &upper);
        }
        if let Some(url) = read_js_string(ctx, *argv.add(1)) {
            set_str(ctx, this_val, "_url", &url);
        }
    }
    set_num(ctx, this_val, "readyState", 1.0);
    sys::js_undefined()
}

/// `setRequestHeader(name, value)` — accumulates into a plain object
/// stored on the XHR instance (`_headers`), applied at `send` time.
pub(super) unsafe extern "C" fn xhr_set_request_header(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "setRequestHeader expects a name and a value");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "header name must be a string");
    };
    let Some(value) = read_js_string(ctx, *argv.add(1)) else {
        return throw_type_error(ctx, "header value must be a string");
    };
    let key = CString::new("_headers").unwrap();
    let mut headers_obj = sys::JS_GetPropertyStr(ctx, this_val, key.as_ptr());
    if headers_obj.tag != sys::JS_TAG_OBJECT {
        sys::JS_FreeValue(ctx, headers_obj);
        headers_obj = sys::JS_NewObject(ctx);
        sys::JS_SetPropertyStr(
            ctx,
            this_val,
            key.as_ptr(),
            sys::JS_DupValue(ctx, headers_obj),
        );
    }
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, headers_obj, cname.as_ptr(), new_js_string(ctx, &value));
    sys::JS_FreeValue(ctx, headers_obj);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn xhr_send(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let url_val = get_prop(ctx, this_val, "_url");
    let Some(url) = read_js_string(ctx, url_val) else {
        sys::JS_FreeValue(ctx, url_val);
        return sys::js_undefined();
    };
    sys::JS_FreeValue(ctx, url_val);

    // Method (uppercased at open() time; default GET when open() never
    // ran or stored nothing), script headers, and the optional body.
    let mut method = "GET".to_string();
    let method_val = get_prop(ctx, this_val, "_method");
    if let Some(stored) = read_js_string(ctx, method_val) {
        method = stored;
    }
    sys::JS_FreeValue(ctx, method_val);
    let mut headers: Vec<(String, String)> = Vec::new();
    let headers_obj = get_prop(ctx, this_val, "_headers");
    if headers_obj.tag == sys::JS_TAG_OBJECT {
        let mut tab: *mut sys::JSPropertyEnum = std::ptr::null_mut();
        let mut len: u32 = 0;
        if sys::JS_GetOwnPropertyNames(
            ctx,
            &mut tab,
            &mut len,
            headers_obj,
            sys::JS_GPN_STRING_ENUM,
        ) >= 0
            && !tab.is_null()
        {
            for entry in std::slice::from_raw_parts(tab, len as usize) {
                let mut name_len: usize = 0;
                let name_ptr = sys::JS_AtomToCStringLen(ctx, &mut name_len, entry.atom);
                if name_ptr.is_null() {
                    continue;
                }
                let bytes = std::slice::from_raw_parts(name_ptr as *const u8, name_len);
                let key = String::from_utf8_lossy(bytes).into_owned();
                sys::JS_FreeCString(ctx, name_ptr);
                let prop_val = sys::JS_GetProperty(ctx, headers_obj, entry.atom);
                if let Some(value) = read_js_string(ctx, prop_val) {
                    headers.push((key, value));
                }
                sys::JS_FreeValue(ctx, prop_val);
            }
            sys::JS_FreePropertyEnum(ctx, tab, len);
        }
    }
    sys::JS_FreeValue(ctx, headers_obj);
    let body: Option<Vec<u8>> = if argc >= 1 {
        match *argv {
            val if val.tag == sys::JS_TAG_STRING => {
                read_js_string(ctx, val).map(|text| text.into_bytes())
            }
            val => crate::form_data::serialize(ctx, val).map(|serialized| {
                if !headers
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                {
                    headers.push(("Content-Type".to_string(), serialized.content_type));
                }
                serialized.body
            }),
        }
    } else {
        None
    };

    if crate::cors::is_mixed_content_blocked(crate::cors::page_origin(ctx).as_deref(), &url)
        || crate::csp::is_request_blocked(ctx, &url)
    {
        // Fed straight to `pump`'s existing completion handling (never
        // actually sent over the network) so a blocked request goes
        // through the exact same readyState/onerror flow a real network
        // failure would, rather than a separate synchronous error path.
        let (tx, rx) = mpsc::channel();
        let _ = tx.send(Err(net::Error::Request(
            "blocked by mixed-content or Content-Security-Policy".to_string(),
        )));
        let xhr_obj = sys::JS_DupValue(ctx, this_val);
        with_state(ctx, |s| {
            s.xhrs.push(PendingXhr {
                xhr_obj,
                url,
                receiver: rx,
            })
        });
        return sys::js_undefined();
    }

    // Referrer last, same as fetch() — browser-level, not script-settable.
    if let Some(referrer) = crate::cors::referrer_header(ctx, &url) {
        headers.push(("Referer".to_string(), referrer));
    }
    let (tx, rx) = mpsc::channel();
    let request_url = url.clone();
    thread::spawn(move || {
        let _ = tx.send(net::request(&method, &url, &headers, body));
    });

    let xhr_obj = sys::JS_DupValue(ctx, this_val);
    with_state(ctx, |s| {
        s.xhrs.push(PendingXhr {
            xhr_obj,
            url: request_url,
            receiver: rx,
        })
    });
    sys::js_undefined()
}
