//! `Element.classList`: a live, identity-cached `DOMTokenList`-shaped object
//! (`CLASS_LIST_OBJECTS`) backed by the node's own `class` attribute, plus
//! the shared `Array.prototype`-borrowing trick (`array_prototype`) that
//! `attributes.rs`'s `NamedNodeMap`-shaped `attributes` collection also
//! reuses.
//!
//! Split into `registry.rs` (the `CLASS_LIST_OBJECTS` identity cache,
//! `cleanup`, `sync_class_list`), `tokens.rs` (`class_tokens`/
//! `valid_class_token`/`array_prototype`), and `mutations.rs`
//! (`add`/`remove`/`contains`/`toggle`/`replace`/`value`) — this file
//! keeps the `classList` getter itself and the public `define_class_list`
//! entry point.

use std::ffi::CString;

use quickjs_sys as sys;

use crate::dom_bindings::node_registry::{dom_opaque, node_id};
use crate::dom_bindings::util::{Getter, Setter};

mod mutations;
mod registry;
mod tokens;

pub(super) use registry::cleanup;
pub(super) use tokens::{array_prototype, class_tokens};

use mutations::{
    class_list_add, class_list_contains, class_list_remove, class_list_replace, class_list_toggle,
    class_list_value_get, class_list_value_set,
};
use registry::{sync_class_list, CLASS_LIST_OBJECTS};

unsafe extern "C" fn node_class_list_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    if let Some(value) = CLASS_LIST_OBJECTS.with(|reg| {
        reg.borrow()
            .get(&(ctx as usize))
            .and_then(|objects| objects.get(&id).copied())
    }) {
        let dom = dom_opaque(ctx);
        if !dom.is_null() {
            sync_class_list(ctx, dom, id, value);
        }
        return sys::JS_DupValue(ctx, value);
    }
    let object = sys::JS_NewObject(ctx);
    let proto = array_prototype(ctx);
    sys::JS_SetPrototype(ctx, object, proto);
    sys::JS_FreeValue(ctx, proto);
    let owner = CString::new("__atomicClassListOwner").unwrap();
    sys::JS_SetPropertyStr(ctx, object, owner.as_ptr(), sys::JS_DupValue(ctx, this_val));
    for (name, function) in [
        ("add", class_list_add as sys::JSCFunction),
        ("remove", class_list_remove as sys::JSCFunction),
        ("contains", class_list_contains as sys::JSCFunction),
        ("toggle", class_list_toggle as sys::JSCFunction),
        ("replace", class_list_replace as sys::JSCFunction),
    ] {
        let name = CString::new(name).unwrap();
        sys::JS_SetPropertyStr(
            ctx,
            object,
            name.as_ptr(),
            sys::JS_NewCFunction2(ctx, function, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0),
        );
    }
    let value_name = CString::new("value").unwrap();
    let value_getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(class_list_value_get),
        value_name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let value_setter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(class_list_value_set),
        value_name.as_ptr(),
        1,
        sys::JS_CFUNC_SETTER,
        0,
    );
    let value_atom = sys::JS_NewAtom(ctx, value_name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        object,
        value_atom,
        value_getter,
        value_setter,
        sys::JS_PROP_HAS_GET | sys::JS_PROP_HAS_SET | sys::JS_PROP_CONFIGURABLE,
    );
    sys::JS_FreeAtom(ctx, value_atom);
    let dom = dom_opaque(ctx);
    if !dom.is_null() {
        sync_class_list(ctx, dom, id, object);
    }
    CLASS_LIST_OBJECTS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, sys::JS_DupValue(ctx, object))
    });
    object
}

pub(super) unsafe fn define_class_list(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("classList").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(node_class_list_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}
