//! `document.getElementById`/`createElement`/`createTextNode`/
//! `createComment`/`createDocumentFragment`/`importNode`/`adoptNode` —
//! node-creation and node-importing methods, split out from `document.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::mutation::valid_tag_name;
use super::node_registry::{dom_opaque, node_class_id_for, node_id, node_object};
use super::util::{read_js_string, throw_type_error, MAX_TEXT_NODE_LENGTH};

pub(super) unsafe extern "C" fn document_get_element_by_id(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_null();
    }
    let Some(id) = read_js_string(ctx, *argv) else {
        return sys::js_null();
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::js_null();
    }
    match (*dom_ptr).find_by_id(&id) {
        Some(node_id) => {
            let class_id = node_class_id_for(ctx, dom_ptr, node_id);
            node_object(ctx, class_id, node_id)
        }
        None => sys::js_null(),
    }
}

pub(super) unsafe extern "C" fn document_create_element(
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
    if !valid_tag_name(&tag) {
        return throw_type_error(ctx, "invalid tag name");
    }
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "document is unavailable");
    }
    let id = (*dom).create_element(&tag);
    let class_id = node_class_id_for(ctx, dom, id);
    node_object(ctx, class_id, id)
}

pub(super) unsafe extern "C" fn document_create_text_node(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "text content is required");
    }
    let Some(text) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "text content must be a string");
    };
    if text.len() > MAX_TEXT_NODE_LENGTH {
        return throw_type_error(ctx, "text node exceeds the maximum length");
    }
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "document is unavailable");
    }
    let id = (*dom).create_text(&text);
    let class_id = node_class_id_for(ctx, dom, id);
    node_object(ctx, class_id, id)
}

pub(super) unsafe extern "C" fn document_create_comment(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "comment data is required");
    }
    let Some(text) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "comment data must be a string");
    };
    if text.len() > MAX_TEXT_NODE_LENGTH {
        return throw_type_error(ctx, "comment exceeds the maximum length");
    }
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "document is unavailable");
    }
    let id = (*dom).create_comment(&text);
    let class_id = node_class_id_for(ctx, dom, id);
    node_object(ctx, class_id, id)
}

pub(super) unsafe extern "C" fn document_create_document_fragment(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "document is unavailable");
    }
    let id = (*dom).create_document_fragment();
    let class_id = node_class_id_for(ctx, dom, id);
    node_object(ctx, class_id, id)
}

/// Real `document.importNode(node, deep?)`: clones a node into this
/// document. Since this engine has a single browsing context (one `Dom`
/// arena), this is equivalent to `cloneNode(deep)` — the `deep` parameter
/// defaults to `true` per the modern spec.
pub(super) unsafe extern "C" fn document_import_node(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "node is required");
    }
    let Some(source_id) = node_id(ctx, *argv) else {
        return throw_type_error(ctx, "argument must be a Node");
    };
    let deep = if argc >= 2 {
        let b = sys::JS_ToBool(ctx, *argv.add(1));
        b >= 0 && b != 0
    } else {
        true
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return throw_type_error(ctx, "document is unavailable");
    }
    let cloned = (*dom).clone_node(source_id, deep);
    let class_id = node_class_id_for(ctx, dom, cloned);
    node_object(ctx, class_id, cloned)
}

/// Real `document.adoptNode(node)`: moves a node into this document.
/// Since this engine has a single browsing context (one `Dom` arena),
/// the node is already in this document — returns it as-is.
pub(super) unsafe extern "C" fn document_adopt_node(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "node is required");
    }
    let Some(id) = node_id(ctx, *argv) else {
        return throw_type_error(ctx, "argument must be a Node");
    };
    let class_id = node_class_id_for(ctx, dom_opaque(ctx), id);
    node_object(ctx, class_id, id)
}
