//! `Headers`, `Request`, and `Response` classes — the fetch API's
//! core object model. Backed by real per-instance data (not plain
//! `{ok, status, body}` objects), so `fetch()`/`fetchSync()` can
//! return proper `Response` instances whose headers are queryable,
//! bodies consumable via `text()`/`json()`/`arrayBuffer()`, and
//! requests cloneable.
//!
//! Deviations: no `ReadableStream` body (body is eagerly buffered),
//! no `FormData` auto-parse on `Response`, no `cache`/`credentials`/`mode`
//! enforcement (fields exist for API shape but are not acted upon),
//! and `bodyUsed` is not tracked (body methods are safe to call once;
//! calling them twice returns the same data, not an error — a real
//! stream would throw after consumption).
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

use crate::js_helpers::{define_getter, define_method};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

unsafe fn set_str(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: &str) {
    let name = CString::new(key).unwrap();
    let js_val = new_js_string(ctx, val);
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), js_val);
}

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str) -> sys::JSValue {
    let name = CString::new(key).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

unsafe fn throw_type_error(ctx: *mut sys::JSContext, msg: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_js_string(ctx, msg))
}

/// Parses `bytes` as JSON via the real `JS_ParseJSON` C API (not a
/// `JSON.parse` source string built and re-entrantly `JS_Eval`'d from
/// inside a native callback — that path was a real, if rare, source of
/// wrong results under `cargo test`'s per-test spawned threads).
unsafe fn parse_json_bytes(ctx: *mut sys::JSContext, bytes: &[u8]) -> Result<sys::JSValue, String> {
    let mut buf = bytes.to_vec();
    buf.push(0);
    let result = sys::JS_ParseJSON(
        ctx,
        buf.as_ptr() as *const std::os::raw::c_char,
        bytes.len(),
        b"<json>\0".as_ptr() as *const std::os::raw::c_char,
    );
    if result.tag == sys::JS_TAG_EXCEPTION {
        sys::JS_FreeValue(ctx, result);
        let exc = sys::JS_GetException(ctx);
        sys::JS_FreeValue(ctx, exc);
        Err("invalid JSON".to_string())
    } else {
        Ok(result)
    }
}

unsafe fn settled_promise(
    ctx: *mut sys::JSContext,
    result: Result<sys::JSValue, String>,
) -> sys::JSValue {
    let mut resolving_funcs = [sys::js_undefined(); 2];
    let promise = sys::JS_NewPromiseCapability(ctx, resolving_funcs.as_mut_ptr());
    let [resolve, reject] = resolving_funcs;
    let (settle_fn, mut arg) = match result {
        Ok(value) => (resolve, value),
        Err(message) => {
            let name = CString::new("Error").unwrap();
            let global = sys::JS_GetGlobalObject(ctx);
            let error_ctor = sys::JS_GetPropertyStr(ctx, global, name.as_ptr());
            sys::JS_FreeValue(ctx, global);
            let mut msg_arg = new_js_string(ctx, &message);
            let error_obj = sys::JS_Call(ctx, error_ctor, sys::js_undefined(), 1, &mut msg_arg);
            sys::JS_FreeValue(ctx, msg_arg);
            sys::JS_FreeValue(ctx, error_ctor);
            (reject, error_obj)
        }
    };
    let settle_result = sys::JS_Call(ctx, settle_fn, sys::js_undefined(), 1, &mut arg);
    sys::JS_FreeValue(ctx, settle_result);
    sys::JS_FreeValue(ctx, arg);
    sys::JS_FreeValue(ctx, resolve);
    sys::JS_FreeValue(ctx, reject);
    promise
}

// ---------------------------------------------------------------------------
// Headers class
// ---------------------------------------------------------------------------

pub(crate) const HEADERS_CLASS_KIND: &str = "Headers";

pub(crate) struct HeadersInner {
    entries: Vec<(String, String)>,
}

unsafe fn headers_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut HeadersInner {
    let class_id = crate::class_registry::class_id_for(rt, HEADERS_CLASS_KIND);
    sys::JS_GetOpaque(this_val, class_id) as *mut HeadersInner
}

unsafe extern "C" fn headers_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = headers_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

/// Find index of header with case-insensitive name match.
fn find_header(entries: &[(String, String)], name: &str) -> Option<usize> {
    entries
        .iter()
        .position(|(n, _)| n.eq_ignore_ascii_case(name))
}

/// Parse a JS init value into header entries. Accepts:
/// - A plain object: `{ "Content-Type": "text/html" }`
/// - An array of arrays: `[["Content-Type", "text/html"]]`
/// - A Headers instance (copies its entries)
unsafe fn parse_headers_init(
    ctx: *mut sys::JSContext,
    init: sys::JSValue,
) -> Vec<(String, String)> {
    let mut entries = Vec::new();

    if init.tag == sys::JS_TAG_UNDEFINED || init.tag == sys::JS_TAG_NULL {
        return entries;
    }

    // Headers instance — copy its entries via opaque
    let opaque = headers_opaque(sys::JS_GetRuntime(ctx), init);
    if !opaque.is_null() {
        return (*opaque).entries.clone();
    }

    if init.tag != sys::JS_TAG_OBJECT {
        return entries;
    }

    // Check if it's an array (check constructor name)
    let ctor_name = get_prop(ctx, init, "constructor");
    if ctor_name.tag == sys::JS_TAG_OBJECT {
        let name_str = get_prop(ctx, ctor_name, "name");
        if let Some(name) = read_js_string(ctx, name_str) {
            if name == "Array" {
                // Array of [name, value] pairs
                let len_val = get_prop(ctx, init, "length");
                let len = if len_val.tag == sys::JS_TAG_INT {
                    len_val.u.int32 as u32
                } else {
                    0
                };
                sys::JS_FreeValue(ctx, len_val);
                for i in 0..len {
                    let pair = sys::JS_GetPropertyUint32(ctx, init, i);
                    if pair.tag == sys::JS_TAG_OBJECT {
                        let name_val = sys::JS_GetPropertyUint32(ctx, pair, 0);
                        let val_val = sys::JS_GetPropertyUint32(ctx, pair, 1);
                        if let (Some(name), Some(value)) =
                            (read_js_string(ctx, name_val), read_js_string(ctx, val_val))
                        {
                            entries.push((name, value));
                        }
                        sys::JS_FreeValue(ctx, name_val);
                        sys::JS_FreeValue(ctx, val_val);
                    }
                    sys::JS_FreeValue(ctx, pair);
                }
                sys::JS_FreeValue(ctx, name_str);
                sys::JS_FreeValue(ctx, ctor_name);
                return entries;
            }
        }
        sys::JS_FreeValue(ctx, name_str);
    }
    sys::JS_FreeValue(ctx, ctor_name);

    // Plain object — enumerate string keys
    let mut tab: *mut sys::JSPropertyEnum = std::ptr::null_mut();
    let mut len: u32 = 0;
    if sys::JS_GetOwnPropertyNames(ctx, &mut tab, &mut len, init, sys::JS_GPN_STRING_ENUM) >= 0
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
            let prop_val = sys::JS_GetProperty(ctx, init, entry.atom);
            if let Some(value) = read_js_string(ctx, prop_val) {
                entries.push((key, value));
            }
            sys::JS_FreeValue(ctx, prop_val);
        }
        sys::JS_FreePropertyEnum(ctx, tab, len);
    }

    entries
}

unsafe extern "C" fn headers_constructor(
    ctx: *mut sys::JSContext,
    _new_target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let entries = if argc >= 1 {
        parse_headers_init(ctx, *argv)
    } else {
        Vec::new()
    };
    let inner = Box::new(HeadersInner { entries });
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::class_id_for(rt, HEADERS_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    sys::JS_SetOpaque(obj, Box::into_raw(inner) as *mut c_void);
    obj
}

/// Resolve a header value by name (case-insensitive). Returns the first
/// matching value, or empty string.
fn headers_get_value(entries: &[(String, String)], name: &str) -> String {
    entries
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

unsafe extern "C" fn headers_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() || argc < 1 {
        return sys::js_undefined();
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let value = headers_get_value(&(*ptr).entries, &name);
    if value.is_empty() {
        sys::js_undefined()
    } else {
        new_js_string(ctx, &value)
    }
}

unsafe extern "C" fn headers_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() || argc < 2 {
        return sys::js_undefined();
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let Some(value) = read_js_string(ctx, *argv.add(1)) else {
        return sys::js_undefined();
    };
    // Remove all existing entries with this name (case-insensitive)
    (*ptr)
        .entries
        .retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
    (*ptr).entries.push((name, value));
    sys::js_undefined()
}

unsafe extern "C" fn headers_append(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() || argc < 2 {
        return sys::js_undefined();
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let Some(value) = read_js_string(ctx, *argv.add(1)) else {
        return sys::js_undefined();
    };
    (*ptr).entries.push((name, value));
    sys::js_undefined()
}

unsafe extern "C" fn headers_delete(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() || argc < 1 {
        return sys::js_undefined();
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    (*ptr)
        .entries
        .retain(|(n, _)| !n.eq_ignore_ascii_case(&name));
    sys::js_undefined()
}

unsafe extern "C" fn headers_has(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() || argc < 1 {
        return sys::js_bool(false);
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return sys::js_bool(false);
    };
    sys::js_bool(find_header(&(*ptr).entries, &name).is_some())
}

unsafe extern "C" fn headers_length_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_float64(0.0);
    }
    // Count unique header names (case-insensitive)
    let mut unique = Vec::new();
    for (name, _) in &(*ptr).entries {
        if !unique.iter().any(|u: &String| u.eq_ignore_ascii_case(name)) {
            unique.push(name.clone());
        }
    }
    sys::js_float64(unique.len() as f64)
}

unsafe extern "C" fn headers_to_string(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return new_js_string(ctx, "");
    }
    let s = (*ptr)
        .entries
        .iter()
        .map(|(n, v)| format!("{n}: {v}"))
        .collect::<Vec<_>>()
        .join("\r\n");
    new_js_string(ctx, &s)
}

unsafe extern "C" fn headers_to_json(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    let obj = sys::JS_NewObject(ctx);
    if ptr.is_null() {
        return obj;
    }
    // Lowercased names, comma-join duplicate values per spec
    for (name, value) in &(*ptr).entries {
        let lower = name.to_ascii_lowercase();
        let existing = get_prop(ctx, obj, &lower);
        let combined = if existing.tag == sys::JS_TAG_STRING {
            if let Some(prev) = read_js_string(ctx, existing) {
                format!("{prev}, {value}")
            } else {
                value.clone()
            }
        } else {
            value.clone()
        };
        sys::JS_FreeValue(ctx, existing);
        set_str(ctx, obj, &lower, &combined);
    }
    obj
}

// --- Iterator helpers for Headers ---

unsafe extern "C" fn headers_entries(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let entries = &(*ptr).entries;
    let len = entries.len() as u32;
    let arr = sys::JS_NewArray(ctx);
    for i in 0..len {
        let pair = sys::JS_NewArray(ctx);
        let (ref name, ref value) = entries[i as usize];
        sys::JS_SetPropertyUint32(ctx, pair, 0, new_js_string(ctx, name));
        sys::JS_SetPropertyUint32(ctx, pair, 1, new_js_string(ctx, value));
        sys::JS_SetPropertyUint32(ctx, arr, i, pair);
    }
    arr
}

unsafe extern "C" fn headers_keys(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let entries = &(*ptr).entries;
    let len = entries.len() as u32;
    let arr = sys::JS_NewArray(ctx);
    for i in 0..len {
        let val = new_js_string(ctx, &entries[i as usize].0);
        sys::JS_SetPropertyUint32(ctx, arr, i, val);
    }
    arr
}

unsafe extern "C" fn headers_values(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let entries = &(*ptr).entries;
    let len = entries.len() as u32;
    let arr = sys::JS_NewArray(ctx);
    for i in 0..len {
        let val = new_js_string(ctx, &entries[i as usize].1);
        sys::JS_SetPropertyUint32(ctx, arr, i, val);
    }
    arr
}

unsafe extern "C" fn headers_for_each(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = headers_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() || argc < 1 {
        return sys::js_undefined();
    }
    let callback = *argv;
    if !sys::JS_IsFunction(ctx, callback) {
        return sys::js_undefined();
    }
    let entries = &(*ptr).entries;
    for (name, value) in entries {
        let mut args = [
            new_js_string(ctx, value),
            new_js_string(ctx, name),
            this_val,
        ];
        let result = sys::JS_Call(ctx, callback, this_val, 3, args.as_mut_ptr());
        sys::JS_FreeValue(ctx, result);
        sys::JS_FreeValue(ctx, args[0]);
        sys::JS_FreeValue(ctx, args[1]);
    }
    sys::js_undefined()
}

// ---------------------------------------------------------------------------
// Request class
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Response class
// ---------------------------------------------------------------------------

pub(crate) const RESPONSE_CLASS_KIND: &str = "Response";

pub(crate) struct ResponseInner {
    pub(crate) status: u16,
    pub(crate) status_text: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Option<Vec<u8>>,
    pub(crate) url: String,
}

unsafe fn response_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut ResponseInner {
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

/// Create a Headers JS object from raw entries — used by `create_response_object`
/// and `request_headers_get`.
pub(crate) unsafe fn create_headers_object(
    ctx: *mut sys::JSContext,
    entries: &[(String, String)],
) -> sys::JSValue {
    let inner = Box::new(HeadersInner {
        entries: entries.to_vec(),
    });
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = crate::class_registry::class_id_for(rt, HEADERS_CLASS_KIND);
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

unsafe extern "C" fn response_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
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

unsafe extern "C" fn response_json(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
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

unsafe extern "C" fn response_array_buffer(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = response_opaque(sys::JS_GetRuntime(ctx), this_val);
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

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Registers `Headers`, `Request`, and `Response` as globals.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    // --- Headers class ---
    let headers_def = sys::JSClassDef {
        class_name: b"Headers\0".as_ptr() as *const std::os::raw::c_char,
        finalizer: Some(headers_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let headers_class_id = crate::class_registry::ensure_class(
        sys::JS_GetRuntime(ctx),
        HEADERS_CLASS_KIND,
        &headers_def,
    );

    let headers_proto = sys::JS_NewObject(ctx);
    define_method(ctx, headers_proto, "get", headers_get, 1);
    define_method(ctx, headers_proto, "set", headers_set, 2);
    define_method(ctx, headers_proto, "append", headers_append, 2);
    define_method(ctx, headers_proto, "delete", headers_delete, 1);
    define_method(ctx, headers_proto, "has", headers_has, 1);
    define_method(ctx, headers_proto, "toString", headers_to_string, 0);
    define_method(ctx, headers_proto, "toJSON", headers_to_json, 0);
    define_method(ctx, headers_proto, "entries", headers_entries, 0);
    define_method(ctx, headers_proto, "keys", headers_keys, 0);
    define_method(ctx, headers_proto, "values", headers_values, 0);
    define_method(ctx, headers_proto, "forEach", headers_for_each, 1);
    define_getter(ctx, headers_proto, "size", headers_length_get);

    let headers_ctor_name = CString::new("Headers").unwrap();
    let headers_ctor = sys::JS_NewCFunction2(
        ctx,
        headers_constructor,
        headers_ctor_name.as_ptr(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        headers_ctor,
        proto_name.as_ptr(),
        sys::JS_DupValue(ctx, headers_proto),
    );
    sys::JS_SetClassProto(ctx, headers_class_id, headers_proto);

    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, headers_ctor_name.as_ptr(), headers_ctor);

    // --- Request class ---
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

    let request_ctor_name = CString::new("Request").unwrap();
    let request_ctor = sys::JS_NewCFunction2(
        ctx,
        request_constructor,
        request_ctor_name.as_ptr(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    sys::JS_SetPropertyStr(
        ctx,
        request_ctor,
        proto_name.as_ptr(),
        sys::JS_DupValue(ctx, request_proto),
    );
    sys::JS_SetClassProto(ctx, request_class_id, request_proto);
    sys::JS_SetPropertyStr(ctx, global, request_ctor_name.as_ptr(), request_ctor);

    // --- Response class ---
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
    define_method(ctx, response_proto, "text", response_text, 0);
    define_method(ctx, response_proto, "json", response_json, 0);
    define_method(ctx, response_proto, "arrayBuffer", response_array_buffer, 0);

    let response_ctor_name = CString::new("Response").unwrap();
    let response_ctor = sys::JS_NewCFunction2(
        ctx,
        response_constructor,
        response_ctor_name.as_ptr(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    sys::JS_SetPropertyStr(
        ctx,
        response_ctor,
        proto_name.as_ptr(),
        sys::JS_DupValue(ctx, response_proto),
    );
    sys::JS_SetClassProto(ctx, response_class_id, response_proto);
    sys::JS_SetPropertyStr(ctx, global, response_ctor_name.as_ptr(), response_ctor);

    sys::JS_FreeValue(ctx, global);
}
