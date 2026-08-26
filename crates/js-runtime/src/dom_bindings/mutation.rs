//! DOM tree mutation methods on `Node.prototype`: attribute get/set/remove,
//! `appendChild`/`insertBefore`/`removeChild`/`replaceChild`/`remove`,
//! `contains`/`compareDocumentPosition`/`normalize`/`cloneNode`, and the
//! `ParentNode`/`ChildNode` variadic helpers (`prepend`/`append`/`before`/
//! `after`/`replaceWith`).

use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

use super::content::insert_adjacent_html;
use super::node_registry::{dom_opaque, node_class_id_for, node_id};
use super::util::{read_js_string, throw_type_error, MAX_ATTRIBUTE_VALUE_LENGTH};

const MAX_TAG_NAME_LENGTH: usize = 64;
const MAX_ATTRIBUTE_NAME_LENGTH: usize = super::util::MAX_ATTRIBUTE_NAME_LENGTH;
/// Bounds the variadic node/string argument list `prepend`/`append`/
/// `before`/`after`/`replaceWith` each accept — an unbounded arg count from
/// a script would otherwise let one call insert an unbounded number of
/// children in one shot, the same class of concern every other bound in
/// this file guards against.
const MAX_CHILD_NODE_ARGS: usize = 256;
const MAX_TEXT_NODE_LENGTH: usize = super::util::MAX_TEXT_NODE_LENGTH;

pub(super) fn valid_tag_name(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= MAX_TAG_NAME_LENGTH
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_attribute_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_ATTRIBUTE_NAME_LENGTH
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':'))
}

fn is_ancestor(dom: &dom::Dom, ancestor: dom::NodeId, mut node: dom::NodeId) -> bool {
    loop {
        if node == ancestor {
            return true;
        }
        let Some(parent) = dom.get(node).and_then(|node| node.parent) else {
            return false;
        };
        node = parent;
    }
}

unsafe extern "C" fn node_append_child(
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
    sys::JS_DupValue(ctx, child_value)
}

unsafe extern "C" fn node_get_attribute(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "attribute name is required");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "attribute name must be a string");
    };
    if !valid_attribute_name(&name) {
        return throw_type_error(ctx, "invalid attribute name");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_null();
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() {
        return sys::js_null();
    }
    (*dom)
        .attribute(id, &name)
        .map(|value| super::util::new_js_string(ctx, value))
        .unwrap_or_else(sys::js_null)
}

unsafe extern "C" fn node_has_attribute(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "attribute name is required");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "attribute name must be a string");
    };
    if !valid_attribute_name(&name) {
        return throw_type_error(ctx, "invalid attribute name");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return sys::js_bool(false);
    };
    let dom = dom_opaque(ctx);
    sys::js_bool(!dom.is_null() && (*dom).attribute(id, &name).is_some())
}

unsafe extern "C" fn node_get_attribute_names(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let array = sys::JS_NewArray(ctx);
    let Some(id) = node_id(ctx, this_val) else {
        return array;
    };
    let dom = dom_opaque(ctx);
    let Some(dom::NodeData::Element { attributes, .. }) = (!dom.is_null())
        .then(|| (*dom).get(id).map(|node| &node.data))
        .flatten()
    else {
        return array;
    };
    let mut names: Vec<_> = attributes.keys().collect();
    names.sort_unstable();
    for (index, name) in names.into_iter().enumerate() {
        sys::JS_SetPropertyUint32(
            ctx,
            array,
            index as u32,
            super::util::new_js_string(ctx, name),
        );
    }
    array
}

unsafe extern "C" fn node_insert_before(
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
    sys::JS_DupValue(ctx, new_value)
}

unsafe extern "C" fn node_set_attribute(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "attribute name and value are required");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "attribute name must be a string");
    };
    let Some(value) = read_js_string(ctx, *argv.add(1)) else {
        return throw_type_error(ctx, "attribute value must be a string");
    };
    if !valid_attribute_name(&name) || value.len() > MAX_ATTRIBUTE_VALUE_LENGTH {
        return throw_type_error(ctx, "invalid attribute");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).set_attribute(id, &name, &value);
    sys::js_undefined()
}

unsafe extern "C" fn node_remove_attribute(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return throw_type_error(ctx, "attribute name is required");
    }
    let Some(name) = read_js_string(ctx, *argv) else {
        return throw_type_error(ctx, "attribute name must be a string");
    };
    if !valid_attribute_name(&name) {
        return throw_type_error(ctx, "invalid attribute name");
    }
    let Some(id) = node_id(ctx, this_val) else {
        return throw_type_error(ctx, "attribute target must be a node");
    };
    let dom = dom_opaque(ctx);
    if dom.is_null() || (*dom).get(id).is_none() {
        return throw_type_error(ctx, "node is no longer attached to this document");
    }
    (*dom).remove_attribute(id, &name);
    sys::js_undefined()
}

unsafe extern "C" fn node_remove(
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
        (*dom).remove_from_parent(id);
    }
    sys::js_undefined()
}

unsafe extern "C" fn node_remove_child(
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
    (*dom).remove_from_parent(child);
    sys::JS_DupValue(ctx, child_value)
}

/// Real `Node.prototype.contains(other)`: `true` when `other` is this node
/// itself or a descendant of it. A non-`Node` (or missing) argument, or a
/// `this` that isn't attached to a live document, is `false` rather than a
/// thrown error — same permissive style `matches`/`closest` already use for
/// a malformed `this`/argument.
unsafe extern "C" fn node_contains(
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

unsafe extern "C" fn node_compare_document_position(
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

unsafe extern "C" fn node_normalize(
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
unsafe extern "C" fn node_clone_node(
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
unsafe extern "C" fn node_replace_child(
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
    (*dom).replace_child(parent, new_child, old_child);
    sys::JS_DupValue(ctx, old_value)
}

/// Also used by `content.rs`'s `node_insert_adjacent_html` (`afterend` case).
pub(super) fn next_sibling_of(dom: &dom::Dom, id: dom::NodeId) -> Option<dom::NodeId> {
    let parent = dom.get(id)?.parent?;
    let siblings = &dom.get(parent)?.children;
    let index = siblings.iter().position(|&c| c == id)?;
    siblings.get(index + 1).copied()
}

/// Also used by `content.rs`'s `node_insert_adjacent_html` (`afterbegin` case).
pub(super) fn first_child_of(dom: &dom::Dom, id: dom::NodeId) -> Option<dom::NodeId> {
    dom.get(id)?.children.first().copied()
}

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
unsafe extern "C" fn node_prepend(
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
unsafe extern "C" fn node_append(
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
unsafe extern "C" fn node_before(
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
unsafe extern "C" fn node_after(
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
unsafe extern "C" fn node_replace_with(
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

pub(super) unsafe fn define_mutation_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, function, arity) in [
        ("appendChild", node_append_child as sys::JSCFunction, 1),
        ("getAttribute", node_get_attribute as sys::JSCFunction, 1),
        ("hasAttribute", node_has_attribute as sys::JSCFunction, 1),
        (
            "getAttributeNames",
            node_get_attribute_names as sys::JSCFunction,
            0,
        ),
        ("setAttribute", node_set_attribute as sys::JSCFunction, 2),
        (
            "removeAttribute",
            node_remove_attribute as sys::JSCFunction,
            1,
        ),
        ("insertBefore", node_insert_before as sys::JSCFunction, 2),
        ("removeChild", node_remove_child as sys::JSCFunction, 1),
        ("remove", node_remove as sys::JSCFunction, 0),
        ("contains", node_contains as sys::JSCFunction, 1),
        ("cloneNode", node_clone_node as sys::JSCFunction, 0),
        ("replaceChild", node_replace_child as sys::JSCFunction, 2),
        ("prepend", node_prepend as sys::JSCFunction, 0),
        ("append", node_append as sys::JSCFunction, 0),
        ("before", node_before as sys::JSCFunction, 0),
        ("after", node_after as sys::JSCFunction, 0),
        ("replaceWith", node_replace_with as sys::JSCFunction, 0),
        (
            "insertAdjacentHTML",
            insert_adjacent_html as sys::JSCFunction,
            2,
        ),
        (
            "compareDocumentPosition",
            node_compare_document_position as sys::JSCFunction,
            1,
        ),
        ("normalize", node_normalize as sys::JSCFunction, 0),
    ] {
        let name = CString::new(name).unwrap();
        let value = sys::JS_NewCFunction2(
            ctx,
            function,
            name.as_ptr(),
            arity,
            sys::JS_CFUNC_GENERIC,
            0,
        );
        sys::JS_SetPropertyStr(ctx, proto, name.as_ptr(), value);
    }
}
