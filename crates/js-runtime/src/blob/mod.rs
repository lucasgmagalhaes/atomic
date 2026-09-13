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
//!
//! Split into `helpers.rs` (shared string/number JS helpers),
//! `construct.rs` (part-appending, prototype resolution, the two
//! constructors), `methods.rs` (`slice`/`text`/`arrayBuffer`), and
//! `object_url.rs` (`URL.createObjectURL`/`revokeObjectURL`) — this file
//! keeps `BLOB_CLASS_KIND`/`BlobInner`, `blob_opaque`, the `size`/`type`
//! getters, `ensure_blob_class`, and the public `register`.
use std::ffi::CString;

use quickjs_sys as sys;

use crate::js_helpers::{define_getter, define_method};

mod construct;
mod helpers;
mod methods;
mod object_url;

pub(crate) use object_url::cleanup;

/// `Blob` and `File` intentionally share one class/kind - see
/// `crate::class_registry`'s docs. `pub(crate)` so `form_data` can build
/// genuine Blob instances for stored file values.
pub(crate) const BLOB_CLASS_KIND: &str = "Blob";

/// Shared opaque layout of every `Blob`/`File` instance (and of the
/// snapshots `form_data` stores). Fields read directly by `form_data`.
pub(crate) struct BlobInner {
    pub(crate) bytes: Vec<u8>,
    pub(crate) mime: String,
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
        return helpers::new_js_string(ctx, "");
    }
    helpers::new_js_string(ctx, &(*ptr).mime)
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
    define_method(ctx, proto, "slice", methods::blob_slice, 2);
    define_method(ctx, proto, "text", methods::blob_text, 0);
    define_method(ctx, proto, "arrayBuffer", methods::blob_array_buffer, 0);

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

/// Registers `Blob`, `File`, and `URL.createObjectURL`/`revokeObjectURL`
/// as globals.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    ensure_blob_class(ctx, "Blob", construct::blob_constructor);
    ensure_blob_class(ctx, "File", construct::file_constructor);

    let global = sys::JS_GetGlobalObject(ctx);
    let url_obj = sys::JS_NewObject(ctx);

    let create_name = CString::new("createObjectURL").unwrap();
    let create_fn = sys::JS_NewCFunction2(
        ctx,
        object_url::create_object_url,
        create_name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, url_obj, create_name.as_ptr(), create_fn);

    let revoke_name = CString::new("revokeObjectURL").unwrap();
    let revoke_fn = sys::JS_NewCFunction2(
        ctx,
        object_url::revoke_object_url,
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
