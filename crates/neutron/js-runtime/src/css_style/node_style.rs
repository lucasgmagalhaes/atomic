//! Real per-`NodeId` object identity for `element.style`
//! (`sync_indices`/`style_owner`/`node_style_get`) + the public
//! `define_style` entry point — split out from `css_style/mod.rs`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::js_helpers::{Getter, Setter};

use super::accessors::{
    css_text_get, css_text_set, get_property_value, named_get, named_set, remove_property,
    set_property, GetterMagic, SetterMagic,
};
use super::properties::{kebab_to_camel, KEBAB_PROPERTIES};
use super::{array_prototype, dom_opaque, new_string};

thread_local! {
    pub(super) static STYLE_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
    /// Last synced declaration count per `(ctx, NodeId)` style object, so
    /// `sync_indices` knows how many trailing indices (from a shrunk
    /// declaration list) need clearing to `undefined` — same convention
    /// `dom_bindings::CLASS_LIST_LENGTHS`/`ATTRS_LENGTHS` already use.
    pub(super) static STYLE_LENGTHS: RefCell<HashMap<usize, HashMap<dom::NodeId, usize>>> = RefCell::new(HashMap::new());
}

pub(super) unsafe fn style_owner(
    ctx: *mut sys::JSContext,
    value: sys::JSValue,
) -> Option<dom::NodeId> {
    let name = CString::new("__atomicStyleOwner").unwrap();
    let owner = sys::JS_GetPropertyStr(ctx, value, name.as_ptr());
    let id = crate::dom_bindings::node_id(ctx, owner);
    sys::JS_FreeValue(ctx, owner);
    id
}

/// Refreshes a style object's indexed properties (`0`, `1`, ...) — each
/// holding a declared property's own name, real `CSSStyleDeclaration`
/// indexed-access semantics — and `length` from the node's live `style`
/// attribute, clearing any trailing index left over from a longer previous
/// declaration list. Called on every mutation and every `element.style`
/// getter hit, same convention `dom_bindings::sync_class_list` uses.
pub(super) unsafe fn sync_indices(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
    object: sys::JSValue,
) {
    let text = (*dom)
        .attribute(id, "style")
        .unwrap_or_default()
        .to_string();
    let decls = super::declarations::parse_declarations(&text);
    let old_len = STYLE_LENGTHS
        .with(|reg| {
            reg.borrow()
                .get(&(ctx as usize))
                .and_then(|nodes| nodes.get(&id).copied())
        })
        .unwrap_or(0);
    for (index, (name, _)) in decls.iter().enumerate() {
        sys::JS_SetPropertyUint32(ctx, object, index as u32, new_string(ctx, name));
    }
    for index in decls.len()..old_len {
        sys::JS_SetPropertyUint32(ctx, object, index as u32, sys::js_undefined());
    }
    let length_name = CString::new("length").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        object,
        length_name.as_ptr(),
        sys::js_float64(decls.len() as f64),
    );
    STYLE_LENGTHS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, decls.len())
    });
}

unsafe extern "C" fn node_style_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = crate::dom_bindings::node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    if let Some(value) = STYLE_OBJECTS.with(|reg| {
        reg.borrow()
            .get(&(ctx as usize))
            .and_then(|nodes| nodes.get(&id).copied())
    }) {
        let dom = dom_opaque(ctx);
        if !dom.is_null() {
            sync_indices(ctx, dom, id, value);
        }
        return sys::JS_DupValue(ctx, value);
    }

    let object = sys::JS_NewObject(ctx);
    let proto = array_prototype(ctx);
    sys::JS_SetPrototype(ctx, object, proto);
    sys::JS_FreeValue(ctx, proto);

    let owner_name = CString::new("__atomicStyleOwner").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        object,
        owner_name.as_ptr(),
        sys::JS_DupValue(ctx, this_val),
    );

    for (name, function, arity) in [
        ("setProperty", set_property as sys::JSCFunction, 2),
        (
            "getPropertyValue",
            get_property_value as sys::JSCFunction,
            1,
        ),
        ("removeProperty", remove_property as sys::JSCFunction, 1),
    ] {
        let cname = CString::new(name).unwrap();
        sys::JS_SetPropertyStr(
            ctx,
            object,
            cname.as_ptr(),
            sys::JS_NewCFunction2(
                ctx,
                function,
                cname.as_ptr(),
                arity,
                sys::JS_CFUNC_GENERIC,
                0,
            ),
        );
    }

    let css_text_name = CString::new("cssText").unwrap();
    let css_text_getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(css_text_get),
        css_text_name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let css_text_setter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(css_text_set),
        css_text_name.as_ptr(),
        1,
        sys::JS_CFUNC_SETTER,
        0,
    );
    let css_text_atom = sys::JS_NewAtom(ctx, css_text_name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        object,
        css_text_atom,
        css_text_getter,
        css_text_setter,
        sys::JS_PROP_HAS_GET | sys::JS_PROP_HAS_SET | sys::JS_PROP_CONFIGURABLE,
    );
    sys::JS_FreeAtom(ctx, css_text_atom);

    for (index, kebab) in KEBAB_PROPERTIES.iter().enumerate() {
        let camel = kebab_to_camel(kebab);
        let cname = CString::new(camel).unwrap();
        let getter = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<GetterMagic, sys::JSCFunction>(named_get),
            cname.as_ptr(),
            0,
            sys::JS_CFUNC_GETTER_MAGIC,
            index as c_int,
        );
        let setter = sys::JS_NewCFunction2(
            ctx,
            std::mem::transmute::<SetterMagic, sys::JSCFunction>(named_set),
            cname.as_ptr(),
            1,
            sys::JS_CFUNC_SETTER_MAGIC,
            index as c_int,
        );
        let atom = sys::JS_NewAtom(ctx, cname.as_ptr());
        sys::JS_DefinePropertyGetSet(
            ctx,
            object,
            atom,
            getter,
            setter,
            sys::JS_PROP_HAS_GET | sys::JS_PROP_HAS_SET | sys::JS_PROP_CONFIGURABLE,
        );
        sys::JS_FreeAtom(ctx, atom);
    }

    let dom = dom_opaque(ctx);
    if !dom.is_null() {
        sync_indices(ctx, dom, id, object);
    }
    STYLE_OBJECTS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, sys::JS_DupValue(ctx, object))
    });
    object
}

/// Defines the `style` accessor on `proto` (`Node.prototype`) — read-only
/// at the property level (there's no `element.style = "..."` real setter;
/// only `cssText`/individual named properties/`setProperty` mutate it),
/// matching real DOM: `Element.prototype.style` itself has no setter
/// either.
pub(crate) unsafe fn define_style(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("style").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(node_style_get),
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
