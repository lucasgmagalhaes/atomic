//! `add`/`remove`/`contains`/`toggle`/`replace`/`value` — split out from
//! `class_list.rs`.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use crate::dom_bindings::node_registry::{dom_opaque, node_id};
use crate::dom_bindings::util::{new_js_string, read_js_string, throw_type_error};

use super::registry::sync_class_list;
use super::tokens::{class_tokens, valid_class_token};

pub(super) unsafe fn class_list_owner(
    ctx: *mut sys::JSContext,
    value: sys::JSValue,
) -> Option<dom::NodeId> {
    let name = CString::new("__atomicClassListOwner").unwrap();
    let owner = sys::JS_GetPropertyStr(ctx, value, name.as_ptr());
    let id = node_id(ctx, owner);
    sys::JS_FreeValue(ctx, owner);
    id
}

unsafe fn class_list_mutate(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
    add: bool,
) -> sys::JSValue {
    let Some(id) = class_list_owner(ctx, this_val) else {
        return throw_type_error(ctx, "invalid classList receiver");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    let mut tokens = class_tokens((*dom).attribute(id, "class").unwrap_or_default());
    for index in 0..argc {
        let Some(token) = read_js_string(ctx, *argv.add(index as usize)) else {
            return throw_type_error(ctx, "class token must be a string");
        };
        if !valid_class_token(&token) {
            return throw_type_error(ctx, "invalid class token");
        }
        if add {
            if !tokens.contains(&token) {
                tokens.push(token);
            }
        } else {
            tokens.retain(|current| current != &token);
        }
    }
    let value = tokens.join(" ");
    if value.len() > crate::dom_bindings::util::MAX_ATTRIBUTE_VALUE_LENGTH {
        return throw_type_error(ctx, "class attribute exceeds the maximum length");
    }
    (*dom).set_attribute(id, "class", &value);
    sync_class_list(ctx, dom, id, this_val);
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn class_list_add(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    class_list_mutate(ctx, this_val, argc, argv, true)
}
pub(super) unsafe extern "C" fn class_list_remove(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    class_list_mutate(ctx, this_val, argc, argv, false)
}
pub(super) unsafe extern "C" fn class_list_contains(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "class token is required");
    }
    let Some(token) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "class token must be a string");
    };
    if !valid_class_token(&token) {
        return throw_type_error(ctx, "invalid class token");
    }
    let Some(id) = class_list_owner(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    sys::js_bool(
        !dom.is_null()
            && class_tokens((*dom).attribute(id, "class").unwrap_or_default()).contains(&token),
    )
}

pub(super) unsafe extern "C" fn class_list_toggle(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "class token is required");
    }
    let Some(token) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "class token must be a string");
    };
    if !valid_class_token(&token) {
        return throw_type_error(ctx, "invalid class token");
    }
    let Some(id) = class_list_owner(ctx, this_val) else {
        return throw_type_error(ctx, "invalid classList receiver");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    let mut tokens = class_tokens((*dom).attribute(id, "class").unwrap_or_default());
    let present = tokens.contains(&token);
    let force = if argc >= 2 {
        Some(sys::JS_ToBool(ctx, *argv.add(1)) != 0)
    } else {
        None
    };
    let should_be_present = force.unwrap_or(!present);
    if should_be_present && !present {
        tokens.push(token);
    } else if !should_be_present && present {
        tokens.retain(|current| current != &token);
    }
    let value = tokens.join(" ");
    if value.len() > crate::dom_bindings::util::MAX_ATTRIBUTE_VALUE_LENGTH {
        return throw_type_error(ctx, "class attribute exceeds the maximum length");
    }
    (*dom).set_attribute(id, "class", &value);
    sync_class_list(ctx, dom, id, this_val);
    sys::js_bool(should_be_present)
}

pub(super) unsafe extern "C" fn class_list_replace(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "old and new class tokens are required");
    }
    let Some(old_token) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "class token must be a string");
    };
    let Some(new_token) = read_js_string(ctx, *argv.add(1)) else {
        return throw_type_error(ctx, "class token must be a string");
    };
    if !valid_class_token(&old_token) || !valid_class_token(&new_token) {
        return throw_type_error(ctx, "invalid class token");
    }
    let Some(id) = class_list_owner(ctx, this_val) else {
        return throw_type_error(ctx, "invalid classList receiver");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    let mut tokens = class_tokens((*dom).attribute(id, "class").unwrap_or_default());
    let Some(position) = tokens.iter().position(|current| current == &old_token) else {
        return sys::js_bool(false);
    };
    tokens[position] = new_token;
    let value = tokens.join(" ");
    if value.len() > crate::dom_bindings::util::MAX_ATTRIBUTE_VALUE_LENGTH {
        return throw_type_error(ctx, "class attribute exceeds the maximum length");
    }
    (*dom).set_attribute(id, "class", &value);
    sync_class_list(ctx, dom, id, this_val);
    sys::js_bool(true)
}

pub(super) unsafe extern "C" fn class_list_value_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = class_list_owner(ctx, this_val) else {
        return new_js_string(ctx, "");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return new_js_string(ctx, "");
    }
    new_js_string(ctx, (*dom).attribute(id, "class").unwrap_or_default())
}

pub(super) unsafe extern "C" fn class_list_value_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argv: sys::JSValue,
) -> sys::JSValue {
    let Some(id) = class_list_owner(ctx, this_val) else {
        return throw_type_error(ctx, "invalid classList receiver");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    let Some(value) = read_js_string(ctx, argv) else {
        return throw_type_error(ctx, "classList.value must be a string");
    };
    if value.len() > crate::dom_bindings::util::MAX_ATTRIBUTE_VALUE_LENGTH {
        return throw_type_error(ctx, "class attribute exceeds the maximum length");
    }
    (*dom).set_attribute(id, "class", &value);
    sync_class_list(ctx, dom, id, this_val);
    sys::js_undefined()
}
