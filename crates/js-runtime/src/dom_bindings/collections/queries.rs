//! `query_selector_all`/`elements_by_tag_name`/`elements_by_class_name` —
//! split out from `collections.rs`.

use quickjs_sys as sys;

use super::super::node_registry::dom_opaque;
use super::super::selectors::matching_nodes;
use super::super::util::throw_type_error;
use super::live::{live_elements_by_class_name, live_elements_by_tag_name};
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

/// Real *live* `HTMLCollection` - see `live.rs`'s own module doc for why
/// this is now a `Proxy`-backed re-query rather than a one-shot snapshot
/// array (that shape stays real and in use for `querySelectorAll`/
/// `document.forms`/`images`/`links`/`scripts` via `html_collection`/
/// `node_list`, a narrower scope cut this function doesn't touch).
pub(in super::super) unsafe fn elements_by_tag_name(
    ctx: *mut sys::JSContext,
    start: dom::NodeId,
    tag: &str,
    include_start: bool,
) -> sys::JSValue {
    live_elements_by_tag_name(ctx, start, tag, include_start)
}

/// Real *live* `HTMLCollection` - see [`elements_by_tag_name`]'s own doc.
pub(in super::super) unsafe fn elements_by_class_name(
    ctx: *mut sys::JSContext,
    start: dom::NodeId,
    class_name: &str,
    include_start: bool,
) -> sys::JSValue {
    live_elements_by_class_name(ctx, start, class_name, include_start)
}
