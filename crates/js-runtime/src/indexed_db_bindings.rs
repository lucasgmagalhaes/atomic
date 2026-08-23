//! `indexedDB.open(name)` and a real `put`/`get`/`delete`/`clear`/
//! `createObjectStore` surface on the returned handle — backed by
//! `storage::indexed_db::IndexedDb`, real file-persisted storage, real
//! structured-clone values via `value_bridge` (a JS object passed to
//! `put` becomes an actual `storage::value::Value` tree, not a
//! `JSON.stringify`'d string).
//!
//! Deviations from the real API: synchronous, not `IDBOpenDBRequest`/
//! `IDBRequest`-and-events based — same "no event loop yet" reasoning
//! documented on `fetchSync` and `timers`, and the same naming convention
//! doesn't apply here only because `indexedDB.open` is already the real
//! method name (there's no `openSync` to rename to; the whole API is just
//! synchronous under the hood). No versioned schema/`onupgradeneeded`, no
//! transactions, no cursors, no indexes exposed to JS yet - `storage::
//! indexed_db` has real ones (`Cursor`, `KeyRange`, `put_indexed`/
//! `get_by_index`), just not wired to a JS binding in this pass; this is
//! the CRUD subset first.
use std::ffi::CString;
use std::os::raw::{c_int, c_void};
use std::sync::atomic::{AtomicU32, Ordering};

use quickjs_sys as sys;

static DB_CLASS_ID: AtomicU32 = AtomicU32::new(0);

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

unsafe fn db_opaque(this_val: sys::JSValue) -> *mut storage::indexed_db::IndexedDb {
    let class_id = DB_CLASS_ID.load(Ordering::Relaxed);
    sys::JS_GetOpaque(this_val, class_id) as *mut storage::indexed_db::IndexedDb
}

unsafe extern "C" fn db_finalizer(_rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = db_opaque(val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

unsafe fn ensure_db_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let class_id = sys::JS_NewClassID(rt, DB_CLASS_ID.as_ptr());

    let class_name = CString::new("IDBDatabaseHandle").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(db_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    sys::JS_NewClass(rt, class_id, &def);

    let proto = sys::JS_NewObject(ctx);
    define_method(ctx, proto, "createObjectStore", create_object_store, 1);
    define_method(ctx, proto, "put", put, 3);
    define_method(ctx, proto, "get", get, 2);
    define_method(ctx, proto, "delete", delete, 2);
    define_method(ctx, proto, "clear", clear, 1);
    sys::JS_SetClassProto(ctx, class_id, proto);

    class_id
}

unsafe fn define_method(ctx: *mut sys::JSContext, proto: sys::JSValue, name: &str, func: sys::JSCFunction, length: c_int) {
    let name_c = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(ctx, func, name_c.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, proto, name_c.as_ptr(), f);
}

unsafe extern "C" fn create_object_store(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let db_ptr = db_opaque(this_val);
    if db_ptr.is_null() || argc < 1 {
        return sys::js_undefined();
    }
    let Some(store_name) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let _ = (*db_ptr).create_object_store(&store_name);
    sys::js_undefined()
}

unsafe extern "C" fn put(ctx: *mut sys::JSContext, this_val: sys::JSValue, argc: c_int, argv: *mut sys::JSValue) -> sys::JSValue {
    let db_ptr = db_opaque(this_val);
    if db_ptr.is_null() || argc < 3 {
        return sys::js_undefined();
    }
    let Some(store_name) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let Some(key) = read_js_string(ctx, *argv.add(1)) else {
        return sys::js_undefined();
    };
    let value = crate::value_bridge::js_to_storage_value(ctx, *argv.add(2));

    if let Some(store) = (*db_ptr).store_mut(&store_name) {
        let _ = store.put(&key, value);
    }
    sys::js_undefined()
}

unsafe extern "C" fn get(ctx: *mut sys::JSContext, this_val: sys::JSValue, argc: c_int, argv: *mut sys::JSValue) -> sys::JSValue {
    let db_ptr = db_opaque(this_val);
    if db_ptr.is_null() || argc < 2 {
        return sys::js_null();
    }
    let Some(store_name) = read_js_string(ctx, *argv) else {
        return sys::js_null();
    };
    let Some(key) = read_js_string(ctx, *argv.add(1)) else {
        return sys::js_null();
    };

    let Some(store) = (*db_ptr).store(&store_name) else {
        return sys::js_null();
    };
    match store.get(&key) {
        Some(value) => crate::value_bridge::storage_value_to_js(ctx, &value),
        None => sys::js_null(),
    }
}

unsafe extern "C" fn delete(ctx: *mut sys::JSContext, this_val: sys::JSValue, argc: c_int, argv: *mut sys::JSValue) -> sys::JSValue {
    let db_ptr = db_opaque(this_val);
    if db_ptr.is_null() || argc < 2 {
        return sys::js_undefined();
    }
    let Some(store_name) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let Some(key) = read_js_string(ctx, *argv.add(1)) else {
        return sys::js_undefined();
    };
    if let Some(store) = (*db_ptr).store_mut(&store_name) {
        let _ = store.delete(&key);
    }
    sys::js_undefined()
}

unsafe extern "C" fn clear(ctx: *mut sys::JSContext, this_val: sys::JSValue, argc: c_int, argv: *mut sys::JSValue) -> sys::JSValue {
    let db_ptr = db_opaque(this_val);
    if db_ptr.is_null() || argc < 1 {
        return sys::js_undefined();
    }
    let Some(store_name) = read_js_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    if let Some(store) = (*db_ptr).store_mut(&store_name) {
        let _ = store.clear();
    }
    sys::js_undefined()
}

unsafe extern "C" fn indexed_db_open(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_null();
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return sys::js_null();
    };

    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return sys::js_null();
    }
    let Some(storage_dir) = (*state).storage_dir.as_ref() else {
        return sys::js_null();
    };
    let path = storage_dir.join("idb").join(&name);
    let Ok(db) = storage::indexed_db::IndexedDb::open(path) else {
        return sys::js_null();
    };

    let class_id = ensure_db_class(ctx);
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    sys::JS_SetOpaque(obj, Box::into_raw(Box::new(db)) as *mut c_void);
    obj
}

/// Registers the global `indexedDB` object with its `open` method.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    let indexed_db = sys::JS_NewObject(ctx);

    let name = CString::new("open").unwrap();
    let open_fn = sys::JS_NewCFunction2(ctx, indexed_db_open, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, indexed_db, name.as_ptr(), open_fn);

    let global_name = CString::new("indexedDB").unwrap();
    sys::JS_SetPropertyStr(ctx, global, global_name.as_ptr(), indexed_db);

    sys::JS_FreeValue(ctx, global);
}
