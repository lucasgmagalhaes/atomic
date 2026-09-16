//! `document.getElementsByTagName`/`getElementsByClassName`/
//! `querySelector`/`querySelectorAll` — split out from `document.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::collections::{elements_by_class_name, elements_by_tag_name, query_selector_all};
use super::node_registry::dom_opaque;
use super::util::{read_js_string, throw_type_error};

pub(super) unsafe extern "C" fn document_get_elements_by_tag_name(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "tag name is required");
    }
    let Some(tag) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "tag name must be a string");
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::JS_NewArray(ctx);
    }
    elements_by_tag_name(ctx, (*dom_ptr).root(), &tag, true)
}

pub(super) unsafe extern "C" fn document_get_elements_by_class_name(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "class name is required");
    }
    let Some(class_name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "class name must be a string");
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::JS_NewArray(ctx);
    }
    elements_by_class_name(ctx, (*dom_ptr).root(), &class_name, true)
}

pub(super) unsafe extern "C" fn document_query_selector(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "selector is required");
    }
    let Some(selector) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "selector must be a string");
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::js_null();
    }
    let all = query_selector_all(ctx, (*dom_ptr).root(), &selector, true);
    if sys::js_is_exception(&all) {
        return all;
    }
    let first = sys::JS_GetPropertyUint32(ctx, all, 0);
    sys::JS_FreeValue(ctx, all);
    if first.tag == sys::JS_TAG_UNDEFINED {
        sys::JS_FreeValue(ctx, first);
        sys::js_null()
    } else {
        first
    }
}

pub(super) unsafe extern "C" fn document_query_selector_all(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "selector is required");
    }
    let Some(selector) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "selector must be a string");
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::JS_NewArray(ctx);
    }
    query_selector_all(ctx, (*dom_ptr).root(), &selector, true)
}
