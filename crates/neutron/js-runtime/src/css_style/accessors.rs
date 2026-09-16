//! `setProperty`/`getPropertyValue`/`removeProperty`/`cssText`/named
//! kebab-property accessors — split out from `css_style/mod.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::declarations::{get_declaration, write_declaration};
use super::node_style::{style_owner, sync_indices};
use super::properties::KEBAB_PROPERTIES;
use super::{dom_opaque, new_string, read_string, throw_type_error, MAX_STYLE_LENGTH};

pub(super) unsafe extern "C" fn set_property(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "setProperty requires a name and a value");
    }
    let Some(name) = read_string(ctx, *argv) else {
        return throw_type_error(ctx, "property name must be a string");
    };
    let Some(value) = read_string(ctx, *argv.add(1)) else {
        return throw_type_error(ctx, "property value must be a string");
    };
    if name.len() > 128 || value.len() > MAX_STYLE_LENGTH {
        return throw_type_error(ctx, "style property or value exceeds the maximum length");
    }
    let Some(id) = style_owner(ctx, this_val) else {
        return throw_type_error(ctx, "invalid style receiver");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    write_declaration(dom, id, &name, Some(&value));
    sync_indices(ctx, dom, id, this_val);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn get_property_value(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return new_string(ctx, "");
    }
    let Some(name) = read_string(ctx, *argv) else {
        return new_string(ctx, "");
    };
    let Some(id) = style_owner(ctx, this_val) else {
        return new_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return new_string(ctx, "");
    }
    new_string(ctx, &get_declaration(dom, id, &name).unwrap_or_default())
}

pub(super) unsafe extern "C" fn remove_property(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return new_string(ctx, "");
    }
    let Some(name) = read_string(ctx, *argv) else {
        return new_string(ctx, "");
    };
    let Some(id) = style_owner(ctx, this_val) else {
        return new_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return new_string(ctx, "");
    }
    let old = get_declaration(dom, id, &name).unwrap_or_default();
    write_declaration(dom, id, &name, None);
    sync_indices(ctx, dom, id, this_val);
    new_string(ctx, &old)
}

pub(super) type GetterMagic =
    unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue, c_int) -> sys::JSValue;
pub(super) type SetterMagic =
    unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue, sys::JSValue, c_int) -> sys::JSValue;

pub(super) unsafe extern "C" fn css_text_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = style_owner(ctx, this_val) else {
        return new_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return new_string(ctx, "");
    }
    new_string(ctx, (*dom).attribute(id, "style").unwrap_or_default())
}

pub(super) unsafe extern "C" fn css_text_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let Some(text) = read_string(ctx, val) else {
        return throw_type_error(ctx, "cssText must be a string");
    };
    if text.len() > MAX_STYLE_LENGTH {
        return throw_type_error(ctx, "cssText exceeds the maximum length");
    }
    let Some(id) = style_owner(ctx, this_val) else {
        return throw_type_error(ctx, "invalid style receiver");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).set_attribute(id, "style", &text);
    sync_indices(ctx, dom, id, this_val);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn named_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    magic: c_int,
) -> sys::JSValue {
    let Some(&name) = KEBAB_PROPERTIES.get(magic as usize) else {
        return sys::js_undefined();
    };
    let Some(id) = style_owner(ctx, this_val) else {
        return new_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return new_string(ctx, "");
    }
    new_string(ctx, &get_declaration(dom, id, name).unwrap_or_default())
}

pub(super) unsafe extern "C" fn named_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
    magic: c_int,
) -> sys::JSValue {
    let Some(&name) = KEBAB_PROPERTIES.get(magic as usize) else {
        return sys::js_undefined();
    };
    let Some(value) = read_string(ctx, val) else {
        return throw_type_error(ctx, "style property value must be a string");
    };
    if value.len() > MAX_STYLE_LENGTH {
        return throw_type_error(ctx, "style value exceeds the maximum length");
    }
    let Some(id) = style_owner(ctx, this_val) else {
        return throw_type_error(ctx, "invalid style receiver");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    write_declaration(dom, id, name, Some(&value));
    sync_indices(ctx, dom, id, this_val);
    sys::js_undefined()
}
