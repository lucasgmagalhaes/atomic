//! `localStorage`/`sessionStorage`: the real `Storage` method surface
//! (`getItem`/`setItem`/`removeItem`/`clear`/`key`/`.length`) backed by
//! `storage::LocalStorage`, real file-persisted key-value storage (see
//! `Context::with_storage`). One implementation serves both globals via
//! `JS_CFUNC_GENERIC_MAGIC`/`JS_CFUNC_GETTER_MAGIC` (magic `0` =
//! `localStorage`, `1` = `sessionStorage`) rather than duplicating five
//! near-identical functions.
//!
//! Without `Context::with_storage` configuring real storage, both globals
//! are still present (registered unconditionally, same pattern as
//! `document.cookie`/`indexedDB`) but inert: `getItem`/`key` return
//! `null`, `.length` reads `0`, writes are silently dropped.
//!
//! Real spec gap: no bracket/dot property access (`localStorage.foo` or
//! `localStorage["foo"]`) — that needs a `Proxy` or exotic-property hooks
//! this crate doesn't implement; the method API (what every real site
//! that doesn't rely on the property-access sugar already uses) is fully
//! real. `sessionStorage`'s clear-on-tab-close lifetime isn't modeled
//! either — see `storage::LocalStorage`'s own doc.
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

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

unsafe fn read_js_number(v: sys::JSValue) -> f64 {
    match v.tag {
        sys::JS_TAG_FLOAT64 => v.u.float64,
        sys::JS_TAG_INT => v.u.int32 as f64,
        _ => 0.0,
    }
}

/// `magic` selects which store: `0` = `local_storage`, anything else =
/// `session_storage`. Null if `ctx` has no `HostState` (plain
/// `Context::new`) or storage wasn't configured (`Context::with_dom`
/// without `with_storage`). Returns a raw pointer, not a borrowed
/// reference - same convention as `dom_bindings::dom_opaque` - since its
/// real lifetime is tied to the `Context` this unsafe function has no way
/// to express in Rust's borrow checker.
unsafe fn store(ctx: *mut sys::JSContext, magic: c_int) -> *mut storage::LocalStorage {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return std::ptr::null_mut();
    }
    let field = if magic == 0 {
        &mut (*state).local_storage
    } else {
        &mut (*state).session_storage
    };
    field
        .as_mut()
        .map(|s| s as *mut storage::LocalStorage)
        .unwrap_or(std::ptr::null_mut())
}

unsafe extern "C" fn get_item(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
    magic: c_int,
) -> sys::JSValue {
    let s = store(ctx, magic);
    if s.is_null() || argc < 1 {
        return sys::js_null();
    }
    let Some(key) = read_js_string(ctx, *argv) else {
        return sys::js_null();
    };
    match (*s).get(&key) {
        Some(v) => new_js_string(ctx, v),
        None => sys::js_null(),
    }
}

unsafe extern "C" fn set_item(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
    magic: c_int,
) -> sys::JSValue {
    let s = store(ctx, magic);
    if !s.is_null() && argc >= 2 {
        if let (Some(key), Some(value)) = (
            read_js_string(ctx, *argv),
            read_js_string(ctx, *argv.add(1)),
        ) {
            let _ = (*s).set(&key, &value);
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn remove_item(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
    magic: c_int,
) -> sys::JSValue {
    let s = store(ctx, magic);
    if !s.is_null() && argc >= 1 {
        if let Some(key) = read_js_string(ctx, *argv) {
            let _ = (*s).remove(&key);
        }
    }
    sys::js_undefined()
}

unsafe extern "C" fn clear_storage(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
    magic: c_int,
) -> sys::JSValue {
    let s = store(ctx, magic);
    if !s.is_null() {
        let _ = (*s).clear();
    }
    sys::js_undefined()
}

unsafe extern "C" fn key_at(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
    magic: c_int,
) -> sys::JSValue {
    let s = store(ctx, magic);
    if s.is_null() || argc < 1 {
        return sys::js_null();
    }
    let index = read_js_number(*argv) as usize;
    match (*s).keys().nth(index) {
        Some(k) => new_js_string(ctx, k),
        None => sys::js_null(),
    }
}

unsafe extern "C" fn length_getter(
    ctx: *mut sys::JSContext,
    _this: sys::JSValue,
    magic: c_int,
) -> sys::JSValue {
    let s = store(ctx, magic);
    sys::js_float64(if s.is_null() { 0.0 } else { (*s).len() as f64 })
}

type GenericMagic = unsafe extern "C" fn(
    *mut sys::JSContext,
    sys::JSValue,
    c_int,
    *mut sys::JSValue,
    c_int,
) -> sys::JSValue;
type GetterMagic = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue, c_int) -> sys::JSValue;

unsafe fn define_method(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    func: GenericMagic,
    length: c_int,
    magic: c_int,
) {
    let cname = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<GenericMagic, sys::JSCFunction>(func),
        cname.as_ptr(),
        length,
        sys::JS_CFUNC_GENERIC_MAGIC,
        magic,
    );
    sys::JS_SetPropertyStr(ctx, obj, cname.as_ptr(), f);
}

unsafe fn define_length(ctx: *mut sys::JSContext, obj: sys::JSValue, magic: c_int) {
    let name = CString::new("length").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<GetterMagic, sys::JSCFunction>(length_getter),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER_MAGIC,
        magic,
    );
    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        obj,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}

unsafe fn register_storage_object(
    ctx: *mut sys::JSContext,
    global: sys::JSValue,
    name: &str,
    magic: c_int,
) {
    let obj = sys::JS_NewObject(ctx);
    define_method(ctx, obj, "getItem", get_item, 1, magic);
    define_method(ctx, obj, "setItem", set_item, 2, magic);
    define_method(ctx, obj, "removeItem", remove_item, 1, magic);
    define_method(ctx, obj, "clear", clear_storage, 0, magic);
    define_method(ctx, obj, "key", key_at, 1, magic);
    define_length(ctx, obj, magic);
    let cname = CString::new(name).unwrap();
    sys::JS_SetPropertyStr(ctx, global, cname.as_ptr(), obj);
}

/// Registers `localStorage` (magic `0`) and `sessionStorage` (magic `1`)
/// as globals.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let global = sys::JS_GetGlobalObject(ctx);
    register_storage_object(ctx, global, "localStorage", 0);
    register_storage_object(ctx, global, "sessionStorage", 1);
    sys::JS_FreeValue(ctx, global);
}
