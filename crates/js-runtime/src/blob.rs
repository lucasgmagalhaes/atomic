//! `Blob`/`File` and `URL.createObjectURL`/`revokeObjectURL` — real byte
//! storage (a boxed `Vec<u8>` per instance, not a placeholder string),
//! `size`/`type` accessors, `slice()`, and `text()`/`arrayBuffer()`
//! returning real (immediately-resolved) Promises, matching the pattern
//! `fetch_async` already uses for `JS_NewPromiseCapability`.
//!
//! `File` reuses the same class as `Blob` plus two extra own properties
//! (`name`, `lastModified`) rather than a real prototype-chain subclass —
//! simpler, and every method (`slice`/`text`/`arrayBuffer`/getters) is
//! shared since both wrap the same opaque `BlobInner`.
//!
//! Deviations from the real API: `Blob` constructor parts accept only
//! strings, `Uint8Array`s, and other `Blob`/`File` instances — not
//! `ArrayBuffer` directly (no `JS_GetArrayBuffer` binding yet, only
//! `JS_GetUint8Array`). `URL.createObjectURL` registers bytes in an
//! in-process, per-context table (not consulted by `fetch`/`XMLHttpRequest`
//! — a `blob:` URL can't actually be fetched here, only looked up via
//! `URL.revokeObjectURL`'s bookkeeping).
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};
use std::sync::atomic::{AtomicU64, Ordering};

use quickjs_sys as sys;

use crate::js_helpers::{define_getter, define_method};

/// `Blob` and `File` intentionally share one class/kind - see
/// `crate::class_registry`'s docs. `pub(crate)` so `form_data` can build
/// genuine Blob instances for stored file values.
pub(crate) const BLOB_CLASS_KIND: &str = "Blob";

static NEXT_BLOB_URL_ID: AtomicU64 = AtomicU64::new(1);

/// Shared opaque layout of every `Blob`/`File` instance (and of the
/// snapshots `form_data` stores). Fields read directly by `form_data`.
pub(crate) struct BlobInner {
    pub(crate) bytes: Vec<u8>,
    pub(crate) mime: String,
}

thread_local! {
    // Keyed by JSContext pointer, same reasoning as `fetch_async::STATE`.
    static OBJECT_URLS: RefCell<HashMap<usize, HashMap<String, BlobInner>>> = RefCell::new(HashMap::new());
}

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

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str) -> sys::JSValue {
    let name = CString::new(key).unwrap();
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr())
}

unsafe fn set_str(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: &str) {
    let name = CString::new(key).unwrap();
    let js_val = new_js_string(ctx, val);
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), js_val);
}

unsafe fn set_num(ctx: *mut sys::JSContext, obj: sys::JSValue, key: &str, val: f64) {
    let name = CString::new(key).unwrap();
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), sys::js_float64(val));
}

/// Reads a JS number value already tagged `INT`/`FLOAT64` (no string- or
/// object-to-number coercion — no `JS_ToFloat64` binding exists yet, see
/// `value_bridge`'s own raw-tag convention).
unsafe fn read_js_number(val: sys::JSValue) -> Option<f64> {
    match val.tag {
        sys::JS_TAG_INT => Some(val.u.int32 as f64),
        sys::JS_TAG_FLOAT64 => Some(val.u.float64),
        _ => None,
    }
}

pub(crate) unsafe fn blob_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut BlobInner {
    let class_id = crate::class_registry::class_id_for(rt, BLOB_CLASS_KIND);
    sys::JS_GetOpaque(this_val, class_id) as *mut BlobInner
}

unsafe extern "C" fn blob_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = blob_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

unsafe extern "C" fn blob_size_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = blob_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_float64(0.0);
    }
    sys::js_float64((*ptr).bytes.len() as f64)
}

unsafe extern "C" fn blob_type_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let ptr = blob_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return new_js_string(ctx, "");
    }
    new_js_string(ctx, &(*ptr).mime)
}

/// Appends one constructor `parts[i]` element's bytes to `out`. Accepts a
/// string (UTF-8 encoded), a `Uint8Array`, or another `Blob`/`File`
/// instance — anything else is silently skipped (documented deviation,
/// see module docs).
unsafe fn append_part(ctx: *mut sys::JSContext, part: sys::JSValue, out: &mut Vec<u8>) {
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
unsafe fn resolve_prototype(
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
unsafe fn make_blob_object(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
    bytes: Vec<u8>,
    mime: String,
) -> sys::JSValue {
    let class_id = crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), BLOB_CLASS_KIND);
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

unsafe extern "C" fn blob_constructor(
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

unsafe extern "C" fn file_constructor(
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

unsafe extern "C" fn blob_slice(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = blob_opaque(sys::JS_GetRuntime(ctx), this_val);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let total = (*ptr).bytes.len() as i64;
    let start = if argc >= 1 {
        read_js_number(*argv).map(|n| n as i64).unwrap_or(0)
    } else {
        0
    };
    let end = if argc >= 2 {
        read_js_number(*argv.add(1))
            .map(|n| n as i64)
            .unwrap_or(total)
    } else {
        total
    };
    let start = start.clamp(0, total) as usize;
    let end = end.clamp(0, total) as usize;
    let sliced = if start < end {
        (&(*ptr).bytes)[start..end].to_vec()
    } else {
        Vec::new()
    };

    let mime = if argc >= 3 {
        read_js_string(ctx, *argv.add(2)).unwrap_or_default()
    } else {
        (*ptr).mime.clone()
    };
    // Per spec, `slice()` always returns a `Blob`, never a `File` -
    // `resolve_prototype` with `new_target: undefined` always takes the
    // fallback (`globalThis.Blob.prototype`) branch.
    let proto = resolve_prototype(ctx, sys::js_undefined(), "Blob");
    let obj = make_blob_object(ctx, proto, sliced, mime);
    sys::JS_FreeValue(ctx, proto);
    obj
}

unsafe fn resolved_promise(ctx: *mut sys::JSContext, value: sys::JSValue) -> sys::JSValue {
    let mut resolving_funcs = [sys::js_undefined(); 2];
    let promise = sys::JS_NewPromiseCapability(ctx, resolving_funcs.as_mut_ptr());
    let [resolve, reject] = resolving_funcs;
    let mut arg = value;
    let result = sys::JS_Call(ctx, resolve, sys::js_undefined(), 1, &mut arg);
    sys::JS_FreeValue(ctx, result);
    sys::JS_FreeValue(ctx, value);
    sys::JS_FreeValue(ctx, resolve);
    sys::JS_FreeValue(ctx, reject);
    promise
}

unsafe extern "C" fn blob_text(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = blob_opaque(sys::JS_GetRuntime(ctx), this_val);
    let text = if ptr.is_null() {
        String::new()
    } else {
        String::from_utf8_lossy(&(*ptr).bytes).into_owned()
    };
    let js_text = new_js_string(ctx, &text);
    resolved_promise(ctx, js_text)
}

unsafe extern "C" fn blob_array_buffer(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let ptr = blob_opaque(sys::JS_GetRuntime(ctx), this_val);
    let buf = if ptr.is_null() {
        sys::JS_NewArrayBufferCopy(ctx, std::ptr::null(), 0)
    } else {
        sys::JS_NewArrayBufferCopy(ctx, (*ptr).bytes.as_ptr(), (*ptr).bytes.len())
    };
    resolved_promise(ctx, buf)
}

unsafe fn ensure_blob_class(
    ctx: *mut sys::JSContext,
    name: &str,
    ctor: sys::JSCFunction,
) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    // `Blob` and `File` share one class/opaque layout (`BlobInner`) and
    // therefore one registry entry (`BLOB_CLASS_KIND`) - see
    // `crate::class_registry`. `File`'s call (after `Blob`'s, same
    // runtime) is a pure cache hit, doesn't touch `JS_NewClass` again.
    let class_name = CString::new("Blob").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(blob_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, BLOB_CLASS_KIND, &def);

    let proto = sys::JS_NewObject(ctx);
    define_getter(ctx, proto, "size", blob_size_get);
    define_getter(ctx, proto, "type", blob_type_get);
    define_method(ctx, proto, "slice", blob_slice, 2);
    define_method(ctx, proto, "text", blob_text, 0);
    define_method(ctx, proto, "arrayBuffer", blob_array_buffer, 0);

    let ctor_name = CString::new(name).unwrap();
    let ctor_fn = sys::JS_NewCFunction2(
        ctx,
        ctor,
        ctor_name.as_ptr(),
        2,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        ctor_fn,
        proto_name.as_ptr(),
        sys::JS_DupValue(ctx, proto),
    );
    sys::JS_SetClassProto(ctx, class_id, proto);

    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, ctor_name.as_ptr(), ctor_fn);
    sys::JS_FreeValue(ctx, global);

    class_id
}

unsafe extern "C" fn create_object_url(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let ptr = blob_opaque(sys::JS_GetRuntime(ctx), *argv);
    if ptr.is_null() {
        return sys::js_undefined();
    }
    let id = NEXT_BLOB_URL_ID.fetch_add(1, Ordering::Relaxed);
    let url = format!("blob:nimble-internal/{id:016x}");
    let entry = BlobInner {
        bytes: (*ptr).bytes.clone(),
        mime: (*ptr).mime.clone(),
    };
    OBJECT_URLS.with(|m| {
        m.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(url.clone(), entry);
    });
    new_js_string(ctx, &url)
}

unsafe extern "C" fn revoke_object_url(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc >= 1 {
        if let Some(url) = read_js_string(ctx, *argv) {
            OBJECT_URLS.with(|m| {
                if let Some(table) = m.borrow_mut().get_mut(&(ctx as usize)) {
                    table.remove(&url);
                }
            });
        }
    }
    sys::js_undefined()
}

/// Registers `Blob`, `File`, and `URL.createObjectURL`/`revokeObjectURL`
/// as globals.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    ensure_blob_class(ctx, "Blob", blob_constructor);
    ensure_blob_class(ctx, "File", file_constructor);

    let global = sys::JS_GetGlobalObject(ctx);
    let url_obj = sys::JS_NewObject(ctx);

    let create_name = CString::new("createObjectURL").unwrap();
    let create_fn = sys::JS_NewCFunction2(
        ctx,
        create_object_url,
        create_name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, url_obj, create_name.as_ptr(), create_fn);

    let revoke_name = CString::new("revokeObjectURL").unwrap();
    let revoke_fn = sys::JS_NewCFunction2(
        ctx,
        revoke_object_url,
        revoke_name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, url_obj, revoke_name.as_ptr(), revoke_fn);

    let url_name = CString::new("URL").unwrap();
    sys::JS_SetPropertyStr(ctx, global, url_name.as_ptr(), url_obj);

    sys::JS_FreeValue(ctx, global);
}

/// Drops this context's `URL.createObjectURL` table (avoids leaking one
/// entry per `JSContext` pointer across the process, same reasoning as
/// `timers::cleanup`/`fetch_async::cleanup`).
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    OBJECT_URLS.with(|m| {
        m.borrow_mut().remove(&(ctx as usize));
    });
}
