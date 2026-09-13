//! `DOMParser`/`XMLSerializer` globals — split out from `document.rs`.

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::node_registry::{dom_opaque, node_class_id_for, node_id, node_object};
use super::util::{new_js_string, read_js_string, throw_type_error, MAX_HTML_LENGTH};

/// `DOMParser` global — a documented deviation from the spec: instead of a
/// separate `Document`, `parseFromString(html, type)` parses into a fresh
/// detached `DocumentFragment` in the shared `Dom`, so the parsed roots can
/// be queried (`fragment.querySelector(...)`) and later adopted/attached.
pub(super) unsafe extern "C" fn dom_parser_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    let name = CString::new("parseFromString").unwrap();
    let function = sys::JS_NewCFunction2(
        ctx,
        dom_parser_parse_from_string,
        name.as_ptr(),
        2,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), function);
    obj
}

unsafe extern "C" fn dom_parser_parse_from_string(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(html) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "parseFromString expects an HTML string");
    };
    if html.len() > MAX_HTML_LENGTH {
        return throw_type_error(ctx, "HTML exceeds the maximum length");
    }
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "no document to parse into");
    }
    let (fragment, roots) = html::parse_fragment(&html);
    let container = (*dom).create_document_fragment();
    for root in roots {
        let cloned = (*dom).adopt(&fragment, root);
        (*dom).append_child(container, cloned);
    }
    let class_id = node_class_id_for(ctx, dom, container);
    node_object(ctx, class_id, container)
}

/// `XMLSerializer` global — `serializeToString(node)` returns
/// `dom::Dom::serialize_node`'s HTML serialization of any node.
pub(super) unsafe extern "C" fn xml_serializer_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let obj = sys::JS_NewObject(ctx);
    let name = CString::new("serializeToString").unwrap();
    let function = sys::JS_NewCFunction2(
        ctx,
        xml_serializer_serialize_to_string,
        name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr(), function);
    obj
}

unsafe extern "C" fn xml_serializer_serialize_to_string(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "serializeToString expects a node");
    }
    let Some(id) = node_id(ctx, *argv) else {
        return throw_type_error(ctx, "serializeToString target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    new_js_string(ctx, &(*dom).serialize_node(id))
}
