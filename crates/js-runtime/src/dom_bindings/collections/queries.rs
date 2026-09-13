//! `query_selector_all`/`elements_by_tag_name`/`elements_by_class_name` —
//! split out from `collections.rs`.

use quickjs_sys as sys;

use super::super::node_registry::dom_opaque;
use super::super::selectors::{matching_by_class, matching_by_tag, matching_nodes};
use super::super::util::throw_type_error;
use super::html_collection::html_collection;
use super::node_list::node_list;

pub(in super::super) unsafe fn query_selector_all(
    ctx: *mut sys::JSContext,
    start: dom::NodeId,
    selector: &str,
    include_start: bool,
) -> sys::JSValue {
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::JS_NewArray(ctx);
    }
    let nodes = match matching_nodes(&*dom_ptr, start, selector, include_start) {
        Ok(nodes) => nodes,
        Err(message) => return throw_type_error(ctx, message),
    };
    node_list(ctx, nodes)
}

pub(in super::super) unsafe fn elements_by_tag_name(
    ctx: *mut sys::JSContext,
    start: dom::NodeId,
    tag: &str,
    include_start: bool,
) -> sys::JSValue {
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::JS_NewArray(ctx);
    }
    match matching_by_tag(&*dom_ptr, start, tag, include_start) {
        Ok(nodes) => html_collection(ctx, nodes),
        Err(message) => throw_type_error(ctx, message),
    }
}

pub(in super::super) unsafe fn elements_by_class_name(
    ctx: *mut sys::JSContext,
    start: dom::NodeId,
    class_name: &str,
    include_start: bool,
) -> sys::JSValue {
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::JS_NewArray(ctx);
    }
    match matching_by_class(&*dom_ptr, start, class_name, include_start) {
        Ok(nodes) => html_collection(ctx, nodes),
        Err(message) => throw_type_error(ctx, message),
    }
}
