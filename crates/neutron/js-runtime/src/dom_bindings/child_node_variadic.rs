//! `ParentNode`/`ChildNode` variadic helpers (`prepend`/`append`/`before`/
//! `after`/`replaceWith`) — split out from `mutation.rs`.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::mutation::{first_child_of, is_ancestor, next_sibling_of};
use super::node_registry::{dom_opaque, node_id};
use super::util::{read_js_string, throw_type_error};

const MAX_TEXT_NODE_LENGTH: usize = super::util::MAX_TEXT_NODE_LENGTH;
/// Bounds the variadic node/string argument list `prepend`/`append`/
/// `before`/`after`/`replaceWith` each accept — an unbounded arg count from
/// a script would otherwise let one call insert an unbounded number of
/// children in one shot, the same class of concern every other bound in
/// this file guards against.
const MAX_CHILD_NODE_ARGS: usize = 256;

/// Resolves the variadic argument list `prepend`/`append`/`before`/`after`/
/// `replaceWith` all share: each argument is either a real string primitive
/// (`arg.tag == JS_TAG_STRING`, checked before any coercion — `read_js_string`
/// itself would happily stringify a `Node` object via its own `toString`,
/// which is not what a `Node` argument means here) turned into a fresh Text
/// node, or an existing `Node` object reused by id. Bounded by
/// `MAX_CHILD_NODE_ARGS`/`MAX_TEXT_NODE_LENGTH`, same convention every other
/// untrusted-input entry point in this file already follows. On error,
/// returns the already-thrown exception value for the caller to return
/// directly.
unsafe fn read_child_node_args(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> Result<Vec<dom::NodeId>, sys::JSValue> {
    if argc as usize > MAX_CHILD_NODE_ARGS {
        return Err(throw_type_error(ctx, "too many arguments"));
    }
    let mut nodes = Vec::with_capacity(argc.max(0) as usize);
    for i in 0..argc {
        let arg = *argv.add(i as usize);
        if arg.tag == sys::JS_TAG_STRING {
            let Some(text) = read_js_string(ctx, arg) else {
                return Err(throw_type_error(ctx, "invalid string argument"));
            };
            if text.len() > MAX_TEXT_NODE_LENGTH {
                return Err(throw_type_error(
                    ctx,
                    "text argument exceeds the maximum length",
                ));
            }
            nodes.push((*dom).create_text(&text));
        } else if let Some(id) = node_id(ctx, arg) {
            if (*dom).get(id).is_none() {
                return Err(throw_type_error(
                    ctx,
                    "node is no longer attached to this document",
                ));
            }
            nodes.push(id);
        } else {
            return Err(throw_type_error(
                ctx,
                "arguments must be a Node or a string",
            ));
        }
    }
    Ok(nodes)
}

/// Real `ParentNode.prepend(...nodes)`: inserts each argument, in order, as
/// this node's new leading children. A no-op on a `this` that isn't a live
/// node (real spec permissiveness — same degrade-gracefully convention
/// every other accessor here already follows for a malformed `this`).
pub(super) unsafe extern "C" fn node_prepend(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(this_id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(this_id).is_none() {
        return sys::js_undefined();
    }
    let nodes = match read_child_node_args(ctx, dom, argc, argv) {
        Ok(nodes) => nodes,
        Err(exception) => return exception,
    };
    for &node in &nodes {
        if is_ancestor(&*dom, node, this_id) {
            return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
        }
    }
    let first_child = first_child_of(&*dom, this_id);
    for node in nodes {
        match first_child {
            Some(sibling) => (*dom).insert_before(sibling, node),
            None => (*dom).append_child(this_id, node),
        }
    }
    sys::js_undefined()
}

/// Real `ParentNode.append(...nodes)`: inserts each argument, in order, as
/// this node's new trailing children.
pub(super) unsafe extern "C" fn node_append(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(this_id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(this_id).is_none() {
        return sys::js_undefined();
    }
    let nodes = match read_child_node_args(ctx, dom, argc, argv) {
        Ok(nodes) => nodes,
        Err(exception) => return exception,
    };
    for &node in &nodes {
        if is_ancestor(&*dom, node, this_id) {
            return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
        }
    }
    for node in nodes {
        (*dom).append_child(this_id, node);
    }
    sys::js_undefined()
}

/// Real `ChildNode.before(...nodes)`: inserts each argument, in order,
/// immediately before this node in its parent. A no-op if this node has no
/// parent, matching the real spec's own "if parent is null, then return".
pub(super) unsafe extern "C" fn node_before(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(this_id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    let Some(parent) = (*dom).get(this_id).and_then(|node| node.parent) else {
        return sys::js_undefined();
    };
    let nodes = match read_child_node_args(ctx, dom, argc, argv) {
        Ok(nodes) => nodes,
        Err(exception) => return exception,
    };
    for &node in &nodes {
        if is_ancestor(&*dom, node, parent) {
            return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
        }
    }
    for node in nodes {
        (*dom).insert_before(this_id, node);
    }
    sys::js_undefined()
}

/// Real `ChildNode.after(...nodes)`: inserts each argument, in order,
/// immediately after this node in its parent. The anchor (this node's
/// original next sibling, if any) is captured once before any insertion —
/// every argument lands ahead of it, in argument order.
pub(super) unsafe extern "C" fn node_after(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(this_id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    let Some(parent) = (*dom).get(this_id).and_then(|node| node.parent) else {
        return sys::js_undefined();
    };
    let nodes = match read_child_node_args(ctx, dom, argc, argv) {
        Ok(nodes) => nodes,
        Err(exception) => return exception,
    };
    for &node in &nodes {
        if is_ancestor(&*dom, node, parent) {
            return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
        }
    }
    let reference = next_sibling_of(&*dom, this_id);
    for node in nodes {
        match reference {
            Some(sibling) => (*dom).insert_before(sibling, node),
            None => (*dom).append_child(parent, node),
        }
    }
    sys::js_undefined()
}

/// Real `ChildNode.replaceWith(...nodes)`: removes this node from its
/// parent and inserts each argument, in order, at the position it
/// occupied. A no-op if this node has no parent. This node is unlinked,
/// not destroyed — same non-destructive-removal contract `removeChild`/
/// `remove`/`replaceChild` follow — so it stays a valid, reattachable
/// node afterward.
pub(super) unsafe extern "C" fn node_replace_with(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(this_id) = node_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_undefined();
    }
    let Some(parent) = (*dom).get(this_id).and_then(|node| node.parent) else {
        return sys::js_undefined();
    };
    let nodes = match read_child_node_args(ctx, dom, argc, argv) {
        Ok(nodes) => nodes,
        Err(exception) => return exception,
    };
    for &node in &nodes {
        if is_ancestor(&*dom, node, parent) {
            return throw_type_error(ctx, "cannot insert an ancestor into its descendant");
        }
    }
    let reference = next_sibling_of(&*dom, this_id);
    (*dom).remove_from_parent(this_id);
    for node in nodes {
        if node == this_id {
            // `this` was passed as one of its own replacement nodes —
            // it's still alive (non-destructive removal), but re-inserting
            // it at its own old position is a genuine no-op worth skipping
            // rather than a real replacement.
            continue;
        }
        match reference {
            Some(sibling) => (*dom).insert_before(sibling, node),
            None => (*dom).append_child(parent, node),
        }
    }
    sys::js_undefined()
}
