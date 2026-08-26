//! Real bidirectional conversion between live `JSValue`s and
//! `storage::value::Value` — the actual structured-clone bridge backing
//! `indexedDB` (`indexed_db_bindings`): a JS object passed to `put()`
//! becomes a real `storage::value::Value` tree (walking arrays via
//! `JS_GetLength`/`JS_GetPropertyUint32`, plain objects via
//! `JS_GetOwnPropertyNames`), not a `JSON.stringify` round trip — and
//! `get()`'s result is rebuilt as a live JS object/array on the way back.
//!
//! Also the engine's one real Structured Clone primitive (`ROADMAP.md` P0
//! item 7 / `primitives.md` §12 — the exact "build once, reuse for
//! `postMessage`/`MessageChannel`/Workers/`history.pushState`" case its
//! own YAGNI carve-out names): `js_to_storage_value` then
//! `storage_value_to_js` is a real independent deep copy through an
//! intermediate representation that owns no `JSValue`s, not a `JS_DupValue`
//! reference bump — mutating the source afterward can't be observed
//! through the clone, or vice versa. Exposed to script directly as the
//! global `structuredClone(value)` ([`register`]), and reused natively by
//! [`crate::history`]'s `pushState`/`replaceState` for `state` (closing
//! that module's own previously-documented "kept by reference, not
//! cloned" deviation).
//!
//! Deviations from a true structured clone: no `Date`/`Map`/`Set`/typed
//! arrays/`RegExp` (functions, symbols, and anything else outside
//! null/undefined/bool/number/string/array/plain-object collapses to
//! `Value::Null` — matches `storage::value::Value`'s own scope, this
//! module doesn't invent variants that type can't hold), and no cycle
//! detection (a JS object referencing itself would recurse forever here;
//! real structured clone handles cycles, this doesn't — same scope cut
//! this module already carried before `structuredClone` existed as a
//! global, not a new one introduced for it).
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;
use storage::value::Value;

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

/// Converts a JS array (`val` must already be known to satisfy
/// `JS_IsArray`) into a `Value::Array`, recursing into each element.
unsafe fn js_array_to_value(ctx: *mut sys::JSContext, val: sys::JSValue) -> Value {
    let mut len: i64 = 0;
    sys::JS_GetLength(ctx, val, &mut len);
    let mut items = Vec::with_capacity(len.max(0) as usize);
    for i in 0..len.max(0) as u32 {
        let item = sys::JS_GetPropertyUint32(ctx, val, i);
        items.push(js_to_storage_value(ctx, item));
        sys::JS_FreeValue(ctx, item);
    }
    Value::Array(items)
}

/// Converts a plain JS object (anything object-tagged that isn't an
/// array) into a `Value::Object`, walking its own enumerable string-keyed
/// properties via `JS_GetOwnPropertyNames` - real property enumeration,
/// not a fixed guess at field names.
unsafe fn js_object_to_value(ctx: *mut sys::JSContext, val: sys::JSValue) -> Value {
    let mut tab: *mut sys::JSPropertyEnum = std::ptr::null_mut();
    let mut len: u32 = 0;
    let rc = sys::JS_GetOwnPropertyNames(ctx, &mut tab, &mut len, val, sys::JS_GPN_STRING_ENUM);
    if rc < 0 || tab.is_null() {
        return Value::Object(Vec::new());
    }

    let entries = std::slice::from_raw_parts(tab, len as usize);
    let mut pairs = Vec::with_capacity(entries.len());
    for entry in entries {
        let mut name_len: usize = 0;
        let name_ptr = sys::JS_AtomToCStringLen(ctx, &mut name_len, entry.atom);
        if name_ptr.is_null() {
            continue;
        }
        let bytes = std::slice::from_raw_parts(name_ptr as *const u8, name_len);
        let key = String::from_utf8_lossy(bytes).into_owned();
        sys::JS_FreeCString(ctx, name_ptr);

        let prop_val = sys::JS_GetProperty(ctx, val, entry.atom);
        pairs.push((key, js_to_storage_value(ctx, prop_val)));
        sys::JS_FreeValue(ctx, prop_val);
    }
    // Frees `tab` and every atom in it - not done per-entry above, this is
    // the one call that owns that cleanup (quickjs-ng's own convention).
    sys::JS_FreePropertyEnum(ctx, tab, len);

    Value::Object(pairs)
}

/// Converts any live `JSValue` into a `storage::value::Value` - the
/// "into storage" half of the bridge. Borrows `val`, doesn't free it;
/// callers keep their own ownership discipline as usual.
pub(crate) unsafe fn js_to_storage_value(ctx: *mut sys::JSContext, val: sys::JSValue) -> Value {
    match val.tag {
        sys::JS_TAG_NULL | sys::JS_TAG_UNDEFINED => Value::Null,
        sys::JS_TAG_BOOL => Value::Bool(val.u.int32 != 0),
        sys::JS_TAG_INT => Value::Number(val.u.int32 as f64),
        sys::JS_TAG_FLOAT64 => Value::Number(val.u.float64),
        sys::JS_TAG_STRING => read_js_string(ctx, val)
            .map(Value::String)
            .unwrap_or(Value::Null),
        sys::JS_TAG_OBJECT => {
            if sys::JS_IsArray(val) {
                js_array_to_value(ctx, val)
            } else {
                js_object_to_value(ctx, val)
            }
        }
        _ => Value::Null,
    }
}

/// Real deep clone of `val` — the actual Structured Clone primitive (see
/// this module's own doc). A plain round trip through
/// `storage::value::Value`: `val` is fully read into an owned tree that
/// borrows no `JSValue` at all, so the rebuilt result shares nothing with
/// `val` — every nested object/array is a fresh `JS_NewObject`/`JS_NewArray`,
/// not a duped reference. Borrows `val`, doesn't free it, same as
/// [`js_to_storage_value`]. Returns an owned reference the caller must
/// eventually free, same as every other `JS_New*`-shaped result here.
pub(crate) unsafe fn deep_clone(ctx: *mut sys::JSContext, val: sys::JSValue) -> sys::JSValue {
    storage_value_to_js(ctx, &js_to_storage_value(ctx, val))
}

unsafe extern "C" fn structured_clone(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    deep_clone(ctx, *argv)
}

/// Registers the global `structuredClone(value)` function on `ctx`.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    let fn_name = CString::new("structuredClone").unwrap();
    let func = sys::JS_NewCFunction2(
        ctx,
        structured_clone,
        fn_name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, global, fn_name.as_ptr(), func);
    sys::JS_FreeValue(ctx, global);
}

/// Rebuilds a `storage::value::Value` as a live `JSValue` - the "out of
/// storage" half of the bridge, used by `IDBObjectStore.get`/cursor
/// `value()`. Returns an owned reference the caller must eventually free
/// (or hand off, e.g. as a property value where the receiving object
/// takes ownership) - same convention as every other `JS_New*` in this
/// binding set.
pub(crate) unsafe fn storage_value_to_js(ctx: *mut sys::JSContext, value: &Value) -> sys::JSValue {
    match value {
        Value::Null => sys::js_null(),
        Value::Bool(b) => sys::js_bool(*b),
        Value::Number(n) => sys::js_float64(*n),
        Value::String(s) => new_js_string(ctx, s),
        Value::Array(items) => {
            let arr = sys::JS_NewArray(ctx);
            for (i, item) in items.iter().enumerate() {
                let js_item = storage_value_to_js(ctx, item);
                sys::JS_SetPropertyUint32(ctx, arr, i as u32, js_item);
            }
            arr
        }
        Value::Object(pairs) => {
            let obj = sys::JS_NewObject(ctx);
            for (key, val) in pairs {
                let js_val = storage_value_to_js(ctx, val);
                let name = CString::new(key.as_str()).unwrap_or_default();
                sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), js_val);
            }
            obj
        }
    }
}
