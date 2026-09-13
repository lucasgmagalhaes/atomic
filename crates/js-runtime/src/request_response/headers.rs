//! `Headers` class.

use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

use crate::js_helpers::{define_getter, define_method};

use super::headers_init::parse_headers_init;
use super::{get_prop, new_js_string, read_js_string, set_str};

pub(crate) const HEADERS_CLASS_KIND: &str = "Headers";

pub(crate) struct HeadersInner {
    pub(super) entries: Vec<(String, String)>,
}

pub(super) unsafe fn headers_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut HeadersInner {
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

/// Create a Headers JS object from raw entries — used by `Response`'s own
/// `create_response_object`/`.headers` getter and `Request`'s own
/// `.headers` getter, so both return a fresh `Headers` instance rather
/// than exposing their internal `Vec<(String, String)>` directly.
pub(super) unsafe fn create_headers_object(
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

pub(super) unsafe fn register(ctx: *mut sys::JSContext) {
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
    define_method(
        ctx,
        headers_proto,
        "entries",
        super::headers_iterators::headers_entries,
        0,
    );
    define_method(
        ctx,
        headers_proto,
        "keys",
        super::headers_iterators::headers_keys,
        0,
    );
    define_method(
        ctx,
        headers_proto,
        "values",
        super::headers_iterators::headers_values,
        0,
    );
    define_method(
        ctx,
        headers_proto,
        "forEach",
        super::headers_iterators::headers_for_each,
        1,
    );
    define_getter(ctx, headers_proto, "size", headers_length_get);

    let headers_ctor_name = std::ffi::CString::new("Headers").unwrap();
    let headers_ctor = sys::JS_NewCFunction2(
        ctx,
        headers_constructor,
        headers_ctor_name.as_ptr(),
        1,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let proto_name = std::ffi::CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        headers_ctor,
        proto_name.as_ptr(),
        sys::JS_DupValue(ctx, headers_proto),
    );
    sys::JS_SetClassProto(ctx, headers_class_id, headers_proto);

    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, headers_ctor_name.as_ptr(), headers_ctor);
    sys::JS_FreeValue(ctx, global);
}
