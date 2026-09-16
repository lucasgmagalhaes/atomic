//! Direct attribute reflection: the fixed `id`/`className`/`name`/`type`/
//! `href`/`checked`/`disabled`/`selected`/`htmlFor` accessor pairs on
//! `Node.prototype` — split out from `attributes.rs`.

use crate::js_helpers::define_getter_setter;

use super::super::node_registry::{dom_opaque, node_id};
use super::super::util::{
    new_js_string, read_js_string, throw_type_error, Getter, Setter, MAX_ATTRIBUTE_VALUE_LENGTH,
};

use quickjs_sys as sys;

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

/// Real, independent `.checked` — see `dom::Dom::checked`'s own doc for
/// why this diverges from `boolean_attribute_get`'s attribute-reflection
/// shape (which `.defaultChecked`, `content.rs`, uses instead).
unsafe extern "C" fn node_checked_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    sys::js_bool(!dom.is_null() && (*dom).checked(id))
}
unsafe extern "C" fn node_checked_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).set_checked(id, sys::JS_ToBool(ctx, val) != 0);
    sys::js_undefined()
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
/// Real, independent `.selected` — like `.checked`, not a direct
/// attribute reflection (that's `.defaultSelected`, see
/// `content::define_default_selected`), so `form_reset`'s per-`<select>`
/// restore has a live value to overwrite that's independent of the
/// `selected` attribute it restores from.
unsafe extern "C" fn node_selected_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    sys::js_bool(!dom.is_null() && (*dom).selected(id))
}
unsafe extern "C" fn node_selected_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).set_selected(id, sys::JS_ToBool(ctx, val) != 0);
    sys::js_undefined()
}

pub(in super::super) unsafe fn define_attribute_properties(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
) {
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
