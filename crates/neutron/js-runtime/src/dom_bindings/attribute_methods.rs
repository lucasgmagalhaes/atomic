//! `getAttribute`/`hasAttribute`/`getAttributeNames`/`setAttribute`/
//! `removeAttribute` on `Node.prototype` — split out from `mutation.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_id};
use super::util::{read_js_string, throw_type_error, MAX_ATTRIBUTE_VALUE_LENGTH};

const MAX_ATTRIBUTE_NAME_LENGTH: usize = super::util::MAX_ATTRIBUTE_NAME_LENGTH;

fn valid_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_ATTRIBUTE_NAME_LENGTH
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
}

pub(super) unsafe extern "C" fn node_get_attribute(
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
    if !valid_attribute_name(&name) {
        return throw_type_error(ctx, "invalid attribute name");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_null();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    (*dom)
        .attribute(id, &name)
        .map(|value| super::util::new_js_string(ctx, value))
        .unwrap_or_else(sys::js_null)
}

pub(super) unsafe extern "C" fn node_has_attribute(
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
    if !valid_attribute_name(&name) {
        return throw_type_error(ctx, "invalid attribute name");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    sys::js_bool(!dom.is_null() && (*dom).attribute(id, &name).is_some())
}

pub(super) unsafe extern "C" fn node_get_attribute_names(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let array = sys::JS_NewArray(ctx);
    let Some(id) = node_id(ctx, this_val) else {
        return array;
    };
    let dom = dom_opaque(ctx);
    let Some(dom::NodeData::Element { attributes, .. }) = (!dom.is_null())
        .then(|| (*dom).get(id).map(|node| &node.data))
        .flatten()
    else {
        return array;
    };
    let mut names: Vec<_> = attributes.keys().collect();
    names.sort_unstable();
    for (index, name) in names.into_iter().enumerate() {
        sys::JS_SetPropertyUint32(
            ctx,
            array,
            index as u32,
            super::util::new_js_string(ctx, name),
        );
    }
    array
}

pub(super) unsafe extern "C" fn node_set_attribute(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "attribute name and value are required");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "attribute name must be a string");
    };
    let Some(value) = read_js_string(ctx, *argv.add(1)) else {
        return throw_type_error(ctx, "attribute value must be a string");
    };
    if !valid_attribute_name(&name) || value.len() > MAX_ATTRIBUTE_VALUE_LENGTH {
        return throw_type_error(ctx, "invalid attribute");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).set_attribute(id, &name, &value);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn node_remove_attribute(
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
    if !valid_attribute_name(&name) {
        return throw_type_error(ctx, "invalid attribute name");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).remove_attribute(id, &name);
    sys::js_undefined()
}
