//! `appendChild`/`insertBefore`/`removeChild`/`replaceChild`/`remove`/
//! `contains`/`compareDocumentPosition`/`normalize`/`cloneNode` on
//! `Node.prototype` — split out from `mutation.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::mutation::is_ancestor;
use super::node_registry::{dom_opaque, node_class_id_for, node_id};
use super::util::throw_type_error;

pub(super) unsafe extern "C" fn node_append_child(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "child node is required");
    }
    let Some(parent) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "append target must be a node");
    };
    let child_value = *argv;
    let Some(child) = node_id(ctx, child_value) else {
        return throw_type_error(ctx, "child must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(parent).is_none() || (*dom).get(child).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if is_ancestor(&*dom, child, parent) {
        return throw_type_error(ctx, "cannot append an ancestor into its descendant");
    }
    (*dom).append_child(parent, child);
    crate::custom_elements::maybe_connected(ctx, dom, child);
    sys::JS_DupValue(ctx, child_value)
}

pub(super) unsafe extern "C" fn node_insert_before(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "new node and reference node are required");
    }
    let Some(parent) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "insert target must be a node");
    };
    let new_value = *argv;
    let reference_value = *argv.add(1);
    let (Some(new_node), Some(reference_node)) =
        (node_id(ctx, new_value), node_id(ctx, reference_value))
    else {
        return throw_type_error(ctx, "insert arguments must be nodes");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null()
        || (*dom).get(parent).is_none()
        || (*dom).get(new_node).is_none()
        || (*dom).get(reference_node).is_none()
    {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if (*dom).get(reference_node).and_then(|node| node.parent) != Some(parent) {
        return throw_type_error(ctx, "reference node is not a child of this parent");
    }
    if is_ancestor(&*dom, new_node, parent) {
        return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
    }
    (*dom).insert_before(reference_node, new_node);
    crate::custom_elements::maybe_connected(ctx, dom, new_node);
    sys::JS_DupValue(ctx, new_value)
}

pub(super) unsafe extern "C" fn node_remove(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if !dom.is_null() && (*dom).get(id).is_some() {
        if id == (*dom).root() {
            return throw_type_error(ctx, "document root cannot be removed");
        }
        // Non-destructive: `id` keeps its identity (same `NodeId`, same
        // cached JS wrapper, own listeners intact) and stays reattachable
        // — only unlinked from its parent, not freed. See spec/architecture/
        // primitives.md §4.1; a real `remove()` never destroys the node.
        let was_connected = (*dom).is_connected(id);
        (*dom).remove_from_parent(id);
        crate::custom_elements::maybe_disconnected(ctx, dom, id, was_connected);
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn node_remove_child(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "child node is required");
    }
    let Some(parent) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "remove target must be a node");
    };
    let child_value = *argv;
    let Some(child) = node_id(ctx, child_value) else {
        return throw_type_error(ctx, "child must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(parent).is_none() || (*dom).get(child).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if (*dom).get(child).and_then(|node| node.parent) != Some(parent) {
        return throw_type_error(ctx, "node is not a child of this parent");
    }
    // Non-destructive: `child` remains a valid, reattachable node (same
    // identity, same listeners) — see `node_remove`'s comment above.
    let was_connected = (*dom).is_connected(child);
    (*dom).remove_from_parent(child);
    crate::custom_elements::maybe_disconnected(ctx, dom, child, was_connected);
    sys::JS_DupValue(ctx, child_value)
}

/// Real `Node.prototype.contains(other)`: `true` when `other` is this node
/// itself or a descendant of it. A non-`Node` (or missing) argument, or a
/// `this` that isn't attached to a live document, is `false` rather than a
/// thrown error — same permissive style `matches`/`closest` already use for
/// a malformed `this`/argument.
pub(super) unsafe extern "C" fn node_contains(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    if argc < 1 {
        return sys::js_bool(false);
    }
    let Some(other) = node_id(ctx, *argv) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    sys::js_bool(!dom.is_null() && (*dom).contains(id, other))
}

pub(super) unsafe extern "C" fn node_compare_document_position(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_float64(1.0);
    };
    if argc < 1 {
        return sys::js_float64(1.0);
    }
    let Some(other) = node_id(ctx, *argv) else {
        return sys::js_float64(1.0);
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_float64(1.0);
    }
    sys::js_float64((*dom).compare_document_position(id, other) as f64)
}

pub(super) unsafe extern "C" fn node_normalize(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if !dom.is_null() {
        (*dom).normalize(id);
    }
    sys::js_undefined()
}

/// Real `Node.prototype.cloneNode(deep)`. Returns a brand-new `Node` object
/// via `node_registry::node_object`, establishing its own object identity in
/// the cache — same as `document.createElement`/`createTextNode` do for a
/// freshly created id, not the identity of the node it was cloned from.
pub(super) unsafe extern "C" fn node_clone_node(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "cloneNode target must be a node");
    };
    let deep = argc > 0 && sys::JS_ToBool(ctx, *argv) != 0;
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    let new_id = (*dom).clone_node(id, deep);
    let class_id = node_class_id_for(ctx, dom, new_id);
    super::node_registry::node_object(ctx, class_id, new_id)
}

/// Real `Node.prototype.replaceChild(newChild, oldChild)`. `oldChild` is
/// unlinked, not destroyed — same non-destructive-removal contract
/// `removeChild`/`remove` follow, so its identity/listeners/cached state
/// survive and it stays reattachable. Rejects `newChild` being this node's
/// own ancestor, same `HierarchyRequestError`-style check `appendChild`/
/// `insertBefore` already make.
pub(super) unsafe extern "C" fn node_replace_child(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "new child and old child are required");
    }
    let Some(parent) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "replace target must be a node");
    };
    let new_value = *argv;
    let old_value = *argv.add(1);
    let (Some(new_child), Some(old_child)) = (node_id(ctx, new_value), node_id(ctx, old_value))
    else {
        return throw_type_error(ctx, "replaceChild arguments must be nodes");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null()
        || (*dom).get(parent).is_none()
        || (*dom).get(new_child).is_none()
        || (*dom).get(old_child).is_none()
    {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    if is_ancestor(&*dom, new_child, parent) {
        return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
    }
    let old_child_is_member = (*dom)
        .get(parent)
        .map(|node| node.children.contains(&old_child))
        .unwrap_or(false);
    if !old_child_is_member {
        return throw_type_error(ctx, "old child is not a child of this parent");
    }
    let old_was_connected = (*dom).is_connected(old_child);
    (*dom).replace_child(parent, new_child, old_child);
    crate::custom_elements::maybe_disconnected(ctx, dom, old_child, old_was_connected);
    crate::custom_elements::maybe_connected(ctx, dom, new_child);
    sys::JS_DupValue(ctx, old_value)
}
