//! `Response` class.

use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

use crate::js_helpers::{define_getter, define_method};

use super::headers::create_headers_object;
use super::headers_init::parse_headers_init;
use super::{get_prop, new_js_string, read_js_string};

pub(crate) const RESPONSE_CLASS_KIND: &str = "Response";

pub(crate) struct ResponseInner {
    pub(crate) status: u16,
    pub(crate) status_text: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Option<Vec<u8>>,
    pub(crate) url: String,
}

pub(super) unsafe fn response_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut ResponseInner {
    let class_id = crate::class_registry::class_id_for(rt, RESPONSE_CLASS_KIND);
    sys::JS_GetOpaque(this_val, class_id) as *mut ResponseInner
}

unsafe extern "C" fn response_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = response_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

/// Create a Response JS object from raw data — used by `fetch.rs` and
/// `fetch_async.rs` to return proper Response instances instead of plain
/// `{ok, status, body}` objects.
pub(crate) unsafe fn create_response_object(
    ctx: *mut sys::JSContext,
    status: u16,
    headers: &[(String, String)],
    body: Option<Vec<u8>>,
    url: &str,
) -> sys::JSValue {
    let status_text = match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "",
    };
    let inner = Box::new(ResponseInner {
        status,
        status_text: status_text.to_string(),
        headers: headers.to_vec(),
        body,
        url: url.to_string(),
    });
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::class_id_for(rt, RESPONSE_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    sys::JS_SetOpaque(obj, Box::into_raw(inner) as *mut c_void);
    obj
}

unsafe extern "C" fn response_constructor(
    ctx: *mut sys::JSContext,
    _new_target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let mut body: Option<Vec<u8>> = None;
    let mut status: u16 = 200;
    let mut status_text = "OK".to_string();
    let mut headers_entries: Vec<(String, String)> = Vec::new();
    let url = String::new();

    // First arg: body (string, Uint8Array, ArrayBuffer, Blob, null, or undefined)
    if argc >= 1 {
        let body_val = *argv;
        match body_val.tag {
            sys::JS_TAG_STRING => {
                if let Some(text) = read_js_string(ctx, body_val) {
                    body = Some(text.into_bytes());
                }
            }
            sys::JS_TAG_OBJECT => {
                // Check if it's a Blob
                let blob_ptr = crate::blob::blob_opaque(sys::JS_GetRuntime(ctx), body_val);
                if !blob_ptr.is_null() {
                    body = Some((*blob_ptr).bytes.clone());
                } else {
                    // Try Uint8Array / ArrayBuffer
                    let mut buf_len: usize = 0;
                    let buf_ptr = sys::JS_GetUint8Array(ctx, &mut buf_len, body_val);
                    if !buf_ptr.is_null() {
                        let bytes = std::slice::from_raw_parts(buf_ptr, buf_len).to_vec();
                        body = Some(bytes);
                    } else {
                        // FormData serialization
                        if let Some(serialized) = crate::form_data::serialize(ctx, body_val) {
                            body = Some(serialized.body);
                            if !headers_entries
                                .iter()
                                .any(|(n, _)| n.eq_ignore_ascii_case("content-type"))
                            {
                                headers_entries
                                    .push(("Content-Type".to_string(), serialized.content_type));
                            }
                        }
                    }
                }
            }
            sys::JS_TAG_NULL | sys::JS_TAG_UNDEFINED => {
                body = None;
            }
            _ => {}
        }
    }

    // Second arg: init object
    if argc >= 2 && (*argv.add(1)).tag == sys::JS_TAG_OBJECT {
        let init = *argv.add(1);
        let status_val = get_prop(ctx, init, "status");
        match status_val.tag {
            sys::JS_TAG_INT => status = status_val.u.int32 as u16,
            sys::JS_TAG_FLOAT64 => status = status_val.u.float64 as u16,
            _ => {}
        }
        sys::JS_FreeValue(ctx, status_val);

        let text_val = get_prop(ctx, init, "statusText");
        if let Some(t) = read_js_string(ctx, text_val) {
            status_text = t;
        }
        sys::JS_FreeValue(ctx, text_val);

        let headers_val = get_prop(ctx, init, "headers");
        if headers_val.tag != sys::JS_TAG_UNDEFINED && headers_val.tag != sys::JS_TAG_NULL {
            headers_entries = parse_headers_init(ctx, headers_val);
        }
        sys::JS_FreeValue(ctx, headers_val);
    }

    let inner = Box::new(ResponseInner {
        status,
        status_text,
        headers: headers_entries,
        body,
        url,
    });
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::class_id_for(rt, RESPONSE_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    sys::JS_SetOpaque(obj, Box::into_raw(inner) as *mut c_void);
    obj
}

unsafe extern "C" fn response_ok_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_bool(false);
    }
    sys::js_bool((200..300).contains(&(*ptr).status))
}

unsafe extern "C" fn response_status_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_float64(0.0);
    }
    sys::js_float64((*ptr).status as f64)
}

unsafe extern "C" fn response_status_text_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return new_js_string(ctx, "");
    }
    new_js_string(ctx, &(*ptr).status_text)
}

unsafe extern "C" fn response_headers_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    create_headers_object(ctx, &(*ptr).headers)
}

unsafe extern "C" fn response_url_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return new_js_string(ctx, "");
    }
    new_js_string(ctx, &(*ptr).url)
}

unsafe extern "C" fn response_type_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    new_js_string(ctx, "basic")
}

unsafe extern "C" fn response_body_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    match &(*ptr).body {
        Some(bytes) => {
            let buf = sys::JS_NewArrayBufferCopy(ctx, bytes.as_ptr(), bytes.len());
            buf
        }
        None => sys::js_undefined(),
    }
}

unsafe extern "C" fn response_body_used_get(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    sys::js_bool(false)
}

unsafe extern "C" fn response_clone(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    create_response_object(
        ctx,
        (*ptr).status,
        &(*ptr).headers,
        (*ptr).body.clone(),
        &(*ptr).url,
    )
}

pub(super) unsafe fn register(ctx: *mut sys::JSContext) {
    let response_def = sys::JSClassDef {
        class_name: b"Response\0".as_ptr() as *const std::os::raw::c_char,
        finalizer: Some(response_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let response_class_id = crate::class_registry::ensure_class(
        sys::JS_GetRuntime(ctx),
        RESPONSE_CLASS_KIND,
        &response_def,
    );

    let response_proto = sys::JS_NewObject(ctx);
    define_getter(ctx, response_proto, "ok", response_ok_get);
    define_getter(ctx, response_proto, "status", response_status_get);
    define_getter(ctx, response_proto, "statusText", response_status_text_get);
    define_getter(ctx, response_proto, "headers", response_headers_get);
    define_getter(ctx, response_proto, "url", response_url_get);
    define_getter(ctx, response_proto, "type", response_type_get);
    define_getter(ctx, response_proto, "body", response_body_get);
    define_getter(ctx, response_proto, "bodyUsed", response_body_used_get);
    define_method(ctx, response_proto, "clone", response_clone, 0);
    define_method(
        ctx,
        response_proto,
        "text",
        super::response_body::response_text,
        0,
    );
    define_method(
        ctx,
        response_proto,
        "json",
        super::response_body::response_json,
        0,
    );
    define_method(
        ctx,
        response_proto,
        "arrayBuffer",
        super::response_body::response_array_buffer,
        0,
    );

    let response_ctor_name = std::ffi::CString::new("Response").unwrap();
    let response_ctor = sys::JS_NewCFunction2(
        ctx,
        response_constructor,
        response_ctor_name.as_ptr(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let proto_name = std::ffi::CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        response_ctor,
        proto_name.as_ptr(),
        sys::JS_DupValue(ctx, response_proto),
    );
    sys::JS_SetClassProto(ctx, response_class_id, response_proto);

    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, response_ctor_name.as_ptr(), response_ctor);
    sys::JS_FreeValue(ctx, global);
}
