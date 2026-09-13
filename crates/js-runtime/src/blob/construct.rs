//! `Blob`/`File` construction: part-appending, prototype resolution, and
//! the two constructors — split out from `blob.rs`.

use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

use super::helpers::{get_prop, read_js_string, set_num, set_str};
use super::{blob_opaque, BlobInner};

/// Appends one constructor `parts[i]` element's bytes to `out`. Accepts a
/// string (UTF-8 encoded), a `Uint8Array`, or another `Blob`/`File`
/// instance — anything else is silently skipped (documented deviation,
/// see module docs).
pub(super) unsafe fn append_part(ctx: *mut sys::JSContext, part: sys::JSValue, out: &mut Vec<u8>) {
    if part.tag == sys::JS_TAG_STRING {
        if let Some(s) = read_js_string(ctx, part) {
            out.extend_from_slice(s.as_bytes());
        }
        return;
    }
    if part.tag != sys::JS_TAG_OBJECT {
        return;
    }
    let blob_ptr = blob_opaque(sys::JS_GetRuntime(ctx), part);
    if !blob_ptr.is_null() {
        out.extend_from_slice(&(*blob_ptr).bytes);
        return;
    }
    let mut len: usize = 0;
    let bytes_ptr = sys::JS_GetUint8Array(ctx, &mut len, part);
    if !bytes_ptr.is_null() {
        out.extend_from_slice(std::slice::from_raw_parts(bytes_ptr, len));
    }
}

/// Resolves the prototype a new `Blob`/`File` instance should use.
/// `new_target` (from a real `new Blob(...)`/`new File(...)` call) wins
/// when present; otherwise falls back to `globalThis.<fallback_ctor_name>
/// .prototype` — a real live lookup, not `ctx`'s implicit
/// `JS_NewObjectClass` default proto (`JS_SetClassProto`'s per-context
/// slot). That implicit default proved unreliable under concurrent
/// `Runtime` construction in practice (see `crate::class_registry`'s
/// docs on the sibling class-id race this crate had to fix) — an
/// explicit lookup sidesteps it entirely, and is also the spec-correct
/// behavior anyway: `Blob.prototype.slice()` always returns a `Blob`,
/// never a `File`, which relying on the implicit default (last writer
/// between `ensure_blob_class("Blob", ...)`/`("File", ...)` wins, since
/// both share one class id) got wrong regardless.
pub(super) unsafe fn resolve_prototype(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    fallback_ctor_name: &str,
) -> sys::JSValue {
    if new_target.tag != sys::JS_TAG_UNDEFINED {
        let proto = get_prop(ctx, new_target, "prototype");
        if proto.tag != sys::JS_TAG_UNDEFINED {
            return proto;
        }
        sys::JS_FreeValue(ctx, proto);
    }
    let global = sys::JS_GetGlobalObject(ctx);
    let ctor_name = CString::new(fallback_ctor_name).unwrap();
    let ctor = sys::JS_GetPropertyStr(ctx, global, ctor_name.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let proto_name = CString::new("prototype").unwrap();
    let proto = sys::JS_GetPropertyStr(ctx, ctor, proto_name.as_ptr());
    sys::JS_FreeValue(ctx, ctor);
    proto
}

/// Builds a new `Blob`/`File` instance with `proto` (borrowed - callers
/// keep ownership, same as every other `JS_SetPrototype` use in this
/// crate) as its prototype.
pub(super) unsafe fn make_blob_object(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
    bytes: Vec<u8>,
    mime: String,
) -> sys::JSValue {
    let class_id =
        crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), super::BLOB_CLASS_KIND);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_SetOpaque(
        obj,
        Box::into_raw(Box::new(BlobInner { bytes, mime })) as *mut c_void,
    );
    obj
}

unsafe fn read_options_type(ctx: *mut sys::JSContext, options: sys::JSValue) -> String {
    if options.tag != sys::JS_TAG_OBJECT {
        return String::new();
    }
    let type_val = get_prop(ctx, options, "type");
    let mime = read_js_string(ctx, type_val).unwrap_or_default();
    sys::JS_FreeValue(ctx, type_val);
    mime
}

pub(super) unsafe extern "C" fn blob_constructor(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let mut bytes = Vec::new();
    if argc >= 1 && sys::JS_IsArray(*argv) {
        let mut len: i64 = 0;
        sys::JS_GetLength(ctx, *argv, &mut len);
        for i in 0..len.max(0) as u32 {
            let part = sys::JS_GetPropertyUint32(ctx, *argv, i);
            append_part(ctx, part, &mut bytes);
            sys::JS_FreeValue(ctx, part);
        }
    }
    let mime = if argc >= 2 {
        read_options_type(ctx, *argv.add(1))
    } else {
        String::new()
    };
    let proto = resolve_prototype(ctx, new_target, "Blob");
    let obj = make_blob_object(ctx, proto, bytes, mime);
    sys::JS_FreeValue(ctx, proto);
    obj
}

pub(super) unsafe extern "C" fn file_constructor(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let mut bytes = Vec::new();
    if argc >= 1 && sys::JS_IsArray(*argv) {
        let mut len: i64 = 0;
        sys::JS_GetLength(ctx, *argv, &mut len);
        for i in 0..len.max(0) as u32 {
            let part = sys::JS_GetPropertyUint32(ctx, *argv, i);
            append_part(ctx, part, &mut bytes);
            sys::JS_FreeValue(ctx, part);
        }
    }
    let name = if argc >= 2 {
        read_js_string(ctx, *argv.add(1)).unwrap_or_default()
    } else {
        String::new()
    };
    let mime = if argc >= 3 {
        read_options_type(ctx, *argv.add(2))
    } else {
        String::new()
    };

    let proto = resolve_prototype(ctx, new_target, "File");
    let obj = make_blob_object(ctx, proto, bytes, mime);
    sys::JS_FreeValue(ctx, proto);
    if !sys::js_is_exception(&obj) {
        set_str(ctx, obj, "name", &name);
        set_num(ctx, obj, "lastModified", 0.0);
    }
    obj
}
