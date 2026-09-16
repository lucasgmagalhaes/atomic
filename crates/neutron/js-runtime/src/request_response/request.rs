//! `Request` class.

use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

use crate::js_helpers::{define_getter, define_method};

use super::headers::create_headers_object;
use super::headers_init::parse_headers_init;
use super::{
    get_prop, new_js_string, parse_json_bytes, read_js_string, settled_promise, throw_type_error,
};

pub(crate) const REQUEST_CLASS_KIND: &str = "Request";

pub(crate) struct RequestInner {
    pub(crate) url: String,
    pub(crate) method: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Option<Vec<u8>>,
}

unsafe fn request_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut RequestInner {
    let class_id = crate::class_registry::class_id_for(rt, REQUEST_CLASS_KIND);
    sys::JS_GetOpaque(this_val, class_id) as *mut RequestInner
}

unsafe extern "C" fn request_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = request_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

unsafe extern "C" fn request_constructor(
    ctx: *mut sys::JSContext,
    _new_target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "Request: url argument is required");
    }

    let mut url = String::new();
    let mut method = "GET".to_string();
    let mut headers_entries: Vec<(String, String)> = Vec::new();
    let mut body: Option<Vec<u8>> = None;

    // First arg: string URL or a Request object
    if (*argv).tag == sys::JS_TAG_STRING {
        if let Some(u) = read_js_string(ctx, *argv) {
            url = u;
        }
    } else if (*argv).tag == sys::JS_TAG_OBJECT {
        // Could be a Request instance
        let req_ptr = request_opaque(sys::JS_GetRuntime(ctx), *argv);
        if !req_ptr.is_null() {
            url = (*req_ptr).url.clone();
            method = (*req_ptr).method.clone();
            headers_entries = (*req_ptr).headers.clone();
            body = (*req_ptr).body.clone();
        } else {
            return throw_type_error(
                ctx,
                "Request: first argument must be a URL string or Request object",
            );
        }
    } else {
        return throw_type_error(
            ctx,
            "Request: first argument must be a URL string or Request object",
        );
    }

    // Second arg: init object
    if argc >= 2 && (*argv.add(1)).tag == sys::JS_TAG_OBJECT {
        let init = *argv.add(1);
        let method_val = get_prop(ctx, init, "method");
        if let Some(m) = read_js_string(ctx, method_val) {
            if !m.is_empty() {
                method = m.to_ascii_uppercase();
            }
        }
        sys::JS_FreeValue(ctx, method_val);

        let headers_val = get_prop(ctx, init, "headers");
        if headers_val.tag != sys::JS_TAG_UNDEFINED && headers_val.tag != sys::JS_TAG_NULL {
            headers_entries = parse_headers_init(ctx, headers_val);
        }
        sys::JS_FreeValue(ctx, headers_val);

        let body_val = get_prop(ctx, init, "body");
        match body_val.tag {
            sys::JS_TAG_STRING => {
                if let Some(text) = read_js_string(ctx, body_val) {
                    body = Some(text.into_bytes());
                    // Auto-set Content-Type if not present
                    if !headers_entries
                        .iter()
                        .any(|(n, _)| n.eq_ignore_ascii_case("content-type"))
                    {
                        headers_entries.push((
                            "Content-Type".to_string(),
                            "text/plain;charset=UTF-8".to_string(),
                        ));
                    }
                }
            }
            sys::JS_TAG_OBJECT => {
                if let Some(serialized) = crate::form_data::serialize(ctx, body_val) {
                    if !headers_entries
                        .iter()
                        .any(|(n, _)| n.eq_ignore_ascii_case("content-type"))
                    {
                        headers_entries.push(("Content-Type".to_string(), serialized.content_type));
                    }
                    body = Some(serialized.body);
                }
            }
            _ => {}
        }
        sys::JS_FreeValue(ctx, body_val);
    }

    let inner = Box::new(RequestInner {
        url,
        method,
        headers: headers_entries,
        body,
    });
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::class_id_for(rt, REQUEST_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    sys::JS_SetOpaque(obj, Box::into_raw(inner) as *mut c_void);
    obj
}

unsafe extern "C" fn request_url_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = request_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return new_js_string(ctx, "");
    }
    new_js_string(ctx, &(*ptr).url)
}

unsafe extern "C" fn request_method_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = request_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return new_js_string(ctx, "GET");
    }
    new_js_string(ctx, &(*ptr).method)
}

unsafe extern "C" fn request_headers_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = request_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    // Return a new Headers instance wrapping a copy of this request's headers
    let entries = (*ptr).headers.clone();
    create_headers_object(ctx, &entries)
}

unsafe extern "C" fn request_body_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = request_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    match &(*ptr).body {
        Some(bytes) => {
            // Return a Uint8Array view of the body bytes
            let buf = sys::JS_NewArrayBufferCopy(ctx, bytes.as_ptr(), bytes.len());
            buf
        }
        None => sys::js_undefined(),
    }
}

unsafe extern "C" fn request_body_used_get(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    // Not tracked — always false
    sys::js_bool(false)
}

unsafe extern "C" fn request_clone(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = request_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let inner = Box::new(RequestInner {
        url: (*ptr).url.clone(),
        method: (*ptr).method.clone(),
        headers: (*ptr).headers.clone(),
        body: (*ptr).body.clone(),
    });
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::class_id_for(rt, REQUEST_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    sys::JS_SetOpaque(obj, Box::into_raw(inner) as *mut c_void);
    obj
}

unsafe extern "C" fn request_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = request_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return settled_promise(ctx, Ok(new_js_string(ctx, "")));
    }
    match &(*ptr).body {
        Some(bytes) => {
            let s = String::from_utf8_lossy(bytes).into_owned();
            settled_promise(ctx, Ok(new_js_string(ctx, &s)))
        }
        None => settled_promise(ctx, Ok(new_js_string(ctx, ""))),
    }
}

unsafe extern "C" fn request_json(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = request_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return settled_promise(ctx, Err("body is null".to_string()));
    }
    match &(*ptr).body {
        Some(bytes) => match parse_json_bytes(ctx, bytes) {
            Ok(value) => settled_promise(ctx, Ok(value)),
            Err(message) => settled_promise(ctx, Err(message)),
        },
        None => settled_promise(ctx, Err("body is null".to_string())),
    }
}

unsafe extern "C" fn request_array_buffer(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = request_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return settled_promise(
            ctx,
            Ok(sys::JS_NewArrayBufferCopy(ctx, std::ptr::null(), 0)),
        );
    }
    match &(*ptr).body {
        Some(bytes) => {
            let buf = sys::JS_NewArrayBufferCopy(ctx, bytes.as_ptr(), bytes.len());
            settled_promise(ctx, Ok(buf))
        }
        None => settled_promise(
            ctx,
            Ok(sys::JS_NewArrayBufferCopy(ctx, std::ptr::null(), 0)),
        ),
    }
}

pub(super) unsafe fn register(ctx: *mut sys::JSContext) {
    let request_def = sys::JSClassDef {
        class_name: b"Request\0".as_ptr() as *const std::os::raw::c_char,
        finalizer: Some(request_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let request_class_id = crate::class_registry::ensure_class(
        sys::JS_GetRuntime(ctx),
        REQUEST_CLASS_KIND,
        &request_def,
    );

    let request_proto = sys::JS_NewObject(ctx);
    define_getter(ctx, request_proto, "url", request_url_get);
    define_getter(ctx, request_proto, "method", request_method_get);
    define_getter(ctx, request_proto, "headers", request_headers_get);
    define_getter(ctx, request_proto, "body", request_body_get);
    define_getter(ctx, request_proto, "bodyUsed", request_body_used_get);
    define_method(ctx, request_proto, "clone", request_clone, 0);
    define_method(ctx, request_proto, "text", request_text, 0);
    define_method(ctx, request_proto, "json", request_json, 0);
    define_method(ctx, request_proto, "arrayBuffer", request_array_buffer, 0);

    let request_ctor_name = std::ffi::CString::new("Request").unwrap();
    let request_ctor = sys::JS_NewCFunction2(
        ctx,
        request_constructor,
        request_ctor_name.as_ptr(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let proto_name = std::ffi::CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        request_ctor,
        proto_name.as_ptr(),
        sys::JS_DupValue(ctx, request_proto),
    );
    sys::JS_SetClassProto(ctx, request_class_id, request_proto);

    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, request_ctor_name.as_ptr(), request_ctor);
    sys::JS_FreeValue(ctx, global);
}
