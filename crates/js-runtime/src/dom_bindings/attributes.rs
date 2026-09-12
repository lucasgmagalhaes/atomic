//! Direct attribute reflection: the fixed `id`/`className`/`name`/`type`/
//! `href`/`checked`/`disabled`/`selected`/`htmlFor` accessor pairs on
//! `Node.prototype`, plus the real `attributes`/`NamedNodeMap`-shaped
//! collection (`ATTRS_OBJECTS`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::js_helpers::define_getter_setter;

use super::node_registry::{dom_opaque, node_id};
use super::util::{
    new_js_string, read_js_string, throw_type_error, Getter, Setter, MAX_ATTRIBUTE_VALUE_LENGTH,
};

thread_local! {
    static ATTRS_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
    static ATTRS_LENGTHS: RefCell<HashMap<usize, HashMap<dom::NodeId, usize>>> = RefCell::new(HashMap::new());
}

/// Frees every cached `attributes` collection object for `ctx` — called
/// from `node_registry::cleanup`.
pub(super) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    crate::js_helpers::cleanup_object_cache(&ATTRS_OBJECTS, ctx);
    crate::js_helpers::cleanup_aux_map(&ATTRS_LENGTHS, ctx);
}

unsafe fn attribute_property_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    name: &str,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    new_js_string(ctx, (*dom).attribute(id, name).unwrap_or_default())
}

unsafe fn attribute_property_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
    name: &str,
) -> sys::JSValue {
    let Some(value) = read_js_string(ctx, val) else {
        return throw_type_error(ctx, "attribute value must be a string");
    };
    if value.len() > MAX_ATTRIBUTE_VALUE_LENGTH {
        return throw_type_error(ctx, "attribute value exceeds the maximum length");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).set_attribute(id, name, &value);
    sys::js_undefined()
}

unsafe extern "C" fn node_id_property_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "id")
}
unsafe extern "C" fn node_id_property_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "id")
}
unsafe extern "C" fn node_class_name_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "class")
}
unsafe extern "C" fn node_class_name_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "class")
}
unsafe extern "C" fn node_form_name_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "name")
}
unsafe extern "C" fn node_form_name_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "name")
}
unsafe extern "C" fn node_type_attribute_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "type")
}
unsafe extern "C" fn node_type_attribute_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "type")
}
unsafe extern "C" fn node_href_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "href")
}
unsafe extern "C" fn node_href_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "href")
}
unsafe extern "C" fn node_html_for_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_get(ctx, this_val, "for")
}
unsafe extern "C" fn node_html_for_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    attribute_property_set(ctx, this_val, val, "for")
}

/// Boolean-attribute reflection (`checked`/`disabled`/`selected`): HTML
/// boolean attributes are presence-only — any attribute value (including
/// `""`) means `true`, and `false` means the attribute is absent entirely,
/// not present with a falsy string value. Mirrors `attribute_property_get`/
/// `_set`'s structure but with that different get/set semantics.
unsafe fn boolean_attribute_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    name: &str,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    sys::js_bool(!dom.is_null() && (*dom).attribute(id, name).is_some())
}

unsafe fn boolean_attribute_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
    name: &str,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if sys::JS_ToBool(ctx, val) != 0 {
        (*dom).set_attribute(id, name, "");
    } else {
        (*dom).remove_attribute(id, name);
    }
    sys::js_undefined()
}

unsafe extern "C" fn node_checked_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_get(ctx, this_val, "checked")
}
unsafe extern "C" fn node_checked_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_set(ctx, this_val, val, "checked")
}
unsafe extern "C" fn node_disabled_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_get(ctx, this_val, "disabled")
}
unsafe extern "C" fn node_disabled_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_set(ctx, this_val, val, "disabled")
}
unsafe extern "C" fn node_selected_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_get(ctx, this_val, "selected")
}
unsafe extern "C" fn node_selected_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    boolean_attribute_set(ctx, this_val, val, "selected")
}

pub(super) unsafe fn define_attribute_properties(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, getter, setter) in [
        (
            "id",
            node_id_property_get as Getter,
            node_id_property_set as Setter,
        ),
        (
            "className",
            node_class_name_get as Getter,
            node_class_name_set as Setter,
        ),
        (
            "name",
            node_form_name_get as Getter,
            node_form_name_set as Setter,
        ),
        (
            "type",
            node_type_attribute_get as Getter,
            node_type_attribute_set as Setter,
        ),
        ("href", node_href_get as Getter, node_href_set as Setter),
        (
            "checked",
            node_checked_get as Getter,
            node_checked_set as Setter,
        ),
        (
            "disabled",
            node_disabled_get as Getter,
            node_disabled_set as Setter,
        ),
        (
            "selected",
            node_selected_get as Getter,
            node_selected_set as Setter,
        ),
        (
            "htmlFor",
            node_html_for_get as Getter,
            node_html_for_set as Setter,
        ),
    ] {
        define_getter_setter(ctx, proto, name, getter, setter);
    }
}

unsafe fn attrs_owner(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<dom::NodeId> {
    let name = CString::new("__atomicAttributesOwner").unwrap();
    let owner = sys::JS_GetPropertyStr(ctx, value, name.as_ptr());
    let id = node_id(ctx, owner);
    sys::JS_FreeValue(ctx, owner);
    id
}

unsafe fn make_attr_entry(ctx: *mut sys::JSContext, name: &str, value: &str) -> sys::JSValue {
    let entry = sys::JS_NewObject(ctx);
    let name_key = CString::new("name").unwrap();
    sys::JS_SetPropertyStr(ctx, entry, name_key.as_ptr(), new_js_string(ctx, name));
    let value_key = CString::new("value").unwrap();
    sys::JS_SetPropertyStr(ctx, entry, value_key.as_ptr(), new_js_string(ctx, value));
    entry
}

/// Refreshes an `attributes`/`NamedNodeMap` object's indexed `{name, value}`
/// entries and `length` from every live attribute on `id`, same
/// "diff against the last sync, clear trailing indices" convention
/// `class_list::sync_class_list` already uses for a shrinking token list.
unsafe fn sync_attributes(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
    object: sys::JSValue,
) {
    let Some(dom::NodeData::Element { attributes, .. }) = (*dom).get(id).map(|node| &node.data)
    else {
        return;
    };
    let mut names: Vec<_> = attributes.keys().cloned().collect();
    names.sort_unstable();
    for (index, name) in names.iter().enumerate() {
        let value = attributes.get(name).cloned().unwrap_or_default();
        let entry = make_attr_entry(ctx, name, &value);
        sys::JS_SetPropertyUint32(ctx, object, index as u32, entry);
    }
    let old_len = ATTRS_LENGTHS
        .with(|reg| {
            reg.borrow()
                .get(&(ctx as usize))
                .and_then(|nodes| nodes.get(&id).copied())
        })
        .unwrap_or(0);
    for index in names.len()..old_len {
        sys::JS_SetPropertyUint32(ctx, object, index as u32, sys::js_undefined());
    }
    let length_name = CString::new("length").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        object,
        length_name.as_ptr(),
        sys::js_float64(names.len() as f64),
    );
    ATTRS_LENGTHS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, names.len())
    });
}

unsafe extern "C" fn attrs_get_named_item(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "attribute name is required");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "attribute name must be a string");
    };
    let Some(id) = attrs_owner(ctx, this_val) else {
        return sys::js_null();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    (*dom)
        .attribute(id, &name)
        .map(|value| make_attr_entry(ctx, &name, value))
        .unwrap_or_else(sys::js_null)
}

unsafe extern "C" fn node_attributes_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    if let Some(value) = ATTRS_OBJECTS.with(|reg| {
        reg.borrow()
            .get(&(ctx as usize))
            .and_then(|objects| objects.get(&id).copied())
    }) {
        let dom = dom_opaque(ctx);
        if !dom.is_null() {
            sync_attributes(ctx, dom, id, value);
        }
        return sys::JS_DupValue(ctx, value);
    }
    let object = sys::JS_NewObject(ctx);
    let proto = super::class_list::array_prototype(ctx);
    sys::JS_SetPrototype(ctx, object, proto);
    sys::JS_FreeValue(ctx, proto);
    let owner = CString::new("__atomicAttributesOwner").unwrap();
    sys::JS_SetPropertyStr(ctx, object, owner.as_ptr(), sys::JS_DupValue(ctx, this_val));
    let method_name = CString::new("getNamedItem").unwrap();
    sys::JS_SetPropertyStr(
        ctx,
        object,
        method_name.as_ptr(),
        sys::JS_NewCFunction2(
            ctx,
            attrs_get_named_item,
            method_name.as_ptr(),
            1,
            sys::JS_CFUNC_GENERIC,
            0,
        ),
    );
    let dom = dom_opaque(ctx);
    if !dom.is_null() {
        sync_attributes(ctx, dom, id, object);
    }
    ATTRS_OBJECTS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(id, sys::JS_DupValue(ctx, object))
    });
    object
}

pub(super) unsafe fn define_attributes_collection(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("attributes").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(node_attributes_get),
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
