//! `querySelector(All)`/`getElementsBy*`/`matches`/`closest` and
//! `define_selector_methods` — split out from `collections.rs`.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::super::node_registry::{dom_opaque, node_class_id_for, node_id, node_object};
use super::super::selectors::{node_matches_selector, MAX_SELECTOR_VISITS};
use super::super::util::{read_js_string, throw_type_error};
use super::queries::{elements_by_class_name, elements_by_tag_name, query_selector_all};

unsafe extern "C" fn node_query_selector(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "selector is required");
    }
    let Some(selector) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "selector must be a string");
    };
    let Some(start) = node_id(ctx, this_val) else {
        return sys::js_null();
    };
    let all = query_selector_all(ctx, start, &selector, false);
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

unsafe extern "C" fn node_query_selector_all(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "selector is required");
    }
    let Some(selector) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "selector must be a string");
    };
    let Some(start) = node_id(ctx, this_val) else {
        return sys::JS_NewArray(ctx);
    };
    query_selector_all(ctx, start, &selector, false)
}

unsafe extern "C" fn node_get_elements_by_tag_name(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "tag name is required");
    }
    let Some(tag) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "tag name must be a string");
    };
    let Some(start) = node_id(ctx, this_val) else {
        return sys::JS_NewArray(ctx);
    };
    elements_by_tag_name(ctx, start, &tag, false)
}

unsafe extern "C" fn node_get_elements_by_class_name(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "class name is required");
    }
    let Some(class_name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "class name must be a string");
    };
    let Some(start) = node_id(ctx, this_val) else {
        return sys::JS_NewArray(ctx);
    };
    elements_by_class_name(ctx, start, &class_name, false)
}

unsafe extern "C" fn node_matches(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "selector is required");
    }
    let Some(selector) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "selector must be a string");
    };
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_bool(false);
    }
    match node_matches_selector(&*dom, id, &selector) {
        Ok(matches) => sys::js_bool(matches),
        Err(message) => throw_type_error(ctx, message),
    }
}

unsafe extern "C" fn node_closest(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "selector is required");
    }
    let Some(selector) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "selector must be a string");
    };
    let Some(mut current) = node_id(ctx, this_val) else {
        return sys::js_null();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    for _ in 0..MAX_SELECTOR_VISITS {
        match node_matches_selector(&*dom, current, &selector) {
            Ok(true) => {
                let class_id = node_class_id_for(ctx, dom, current);
                return node_object(ctx, class_id, current);
            }
            Ok(false) => {}
            Err(message) => return throw_type_error(ctx, message),
        }
        let Some(parent) = (*dom).get(current).and_then(|node| node.parent) else {
            return sys::js_null();
        };
        current = parent;
    }
    throw_type_error(ctx, "selector traversal limit exceeded")
}

pub(in super::super) unsafe fn define_selector_methods(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
) {
    for (name, function) in [
        ("querySelector", node_query_selector as sys::JSCFunction),
        (
            "querySelectorAll",
            node_query_selector_all as sys::JSCFunction,
        ),
        ("matches", node_matches as sys::JSCFunction),
        ("closest", node_closest as sys::JSCFunction),
        (
            "getElementsByTagName",
            node_get_elements_by_tag_name as sys::JSCFunction,
        ),
        (
            "getElementsByClassName",
            node_get_elements_by_class_name as sys::JSCFunction,
        ),
    ] {
        let name = CString::new(name).unwrap();
        let value =
            sys::JS_NewCFunction2(ctx, function, name.as_ptr(), 1, sys::JS_CFUNC_GENERIC, 0);
        sys::JS_SetPropertyStr(ctx, proto, name.as_ptr(), value);
    }
}
