//! Shared QuickJS registration helpers used by 3+ binding modules
//! (`blob`, `clipboard`, `fetch_async`, `indexed_db_bindings`, `web_audio`,
//! `url_bindings`, `cssom_stylesheet`, `request_response`, `trusted_types`,
//! ...). Each of these used to hand-roll its own byte-identical
//! `define_method`/`define_getter`/`Getter`/`Setter` copy — factored here
//! per this repo's own DRY rule ("if you're about to write a third
//! near-identical `JS_NewCFunction2`/`JS_DefinePropertyGetSet`
//! registration by hand, factor a helper first").
//!
//! `dom_bindings` has its own parallel `Getter`/`Setter` aliases in
//! `dom_bindings::util` rather than reusing these — that submodule tree
//! is `pub(super)`-scoped and predates this module; re-pointing it here
//! is a separate, larger call-site sweep left for a later pass.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::hash::Hash;
use std::os::raw::c_int;
use std::thread::LocalKey;

use quickjs_sys as sys;

pub(crate) type Getter =
    unsafe extern "C" fn(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue;
pub(crate) type Setter = unsafe extern "C" fn(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue;

/// Registers a plain method (`JS_CFUNC_GENERIC`) on `obj` (a prototype or
/// a plain object, e.g. a constructor's static side).
pub(crate) unsafe fn define_method(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    func: sys::JSCFunction,
    length: c_int,
) {
    let name_c = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(ctx, func, name_c.as_ptr(), length, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, obj, name_c.as_ptr(), f);
}

/// Registers a read-only accessor property (getter only, no setter).
pub(crate) unsafe fn define_getter(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    getter: Getter,
) {
    let name_c = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(getter),
        name_c.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name_c.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        obj,
        atom,
        f,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}

/// Registers a read-write accessor property (both getter and setter).
pub(crate) unsafe fn define_getter_setter(
    ctx: *mut sys::JSContext,
    obj: sys::JSValue,
    name: &str,
    getter: Getter,
    setter: Setter,
) {
    let name_c = CString::new(name).unwrap();
    let g = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(getter),
        name_c.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let s = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(setter),
        name_c.as_ptr(),
        1,
        sys::JS_CFUNC_SETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name_c.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        obj,
        atom,
        g,
        s,
        sys::JS_PROP_HAS_GET
            | sys::JS_PROP_HAS_SET
            | sys::JS_PROP_CONFIGURABLE
            | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}

/// Frees every cached `JSValue` in a per-`(ctx, NodeId)` object-identity
/// registry (`CLASS_LIST_OBJECTS`/`DATASET_OBJECTS`/`ATTRS_OBJECTS`/
/// `STYLE_OBJECTS` all share this exact shape) and drops `ctx`'s own
/// entry — called from each module's `cleanup(ctx)`, itself called from
/// `node_registry::cleanup` before `JS_FreeContext`.
pub(crate) unsafe fn cleanup_object_cache<K: Eq + Hash>(
    registry: &'static LocalKey<RefCell<HashMap<usize, HashMap<K, sys::JSValue>>>>,
    ctx: *mut sys::JSContext,
) {
    if let Some(objects) = registry.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
        for (_, obj) in objects {
            sys::JS_FreeValue(ctx, obj);
        }
    }
}

/// Drops `ctx`'s entry from a per-`(ctx, NodeId)` auxiliary map that needs
/// no `JSValue` freeing (`CLASS_LIST_LENGTHS`/`DATASET_KEYS`/
/// `ATTRS_LENGTHS`/`STYLE_LENGTHS`) — the non-`JSValue` counterpart to
/// [`cleanup_object_cache`], same call-site convention.
pub(crate) fn cleanup_aux_map<K, V>(
    registry: &'static LocalKey<RefCell<HashMap<usize, HashMap<K, V>>>>,
    ctx: *mut sys::JSContext,
) {
    registry.with(|reg| {
        reg.borrow_mut().remove(&(ctx as usize));
    });
}
