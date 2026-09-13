//! DOM tree mutation methods on `Node.prototype`: attribute get/set/remove,
//! `appendChild`/`insertBefore`/`removeChild`/`replaceChild`/`remove`,
//! `contains`/`compareDocumentPosition`/`normalize`/`cloneNode`, and the
//! `ParentNode`/`ChildNode` variadic helpers (`prepend`/`append`/`before`/
//! `after`/`replaceWith`).
//!
//! Split into `attribute_methods.rs` (attribute get/set/remove),
//! `tree_edit.rs` (appendChild/insertBefore/removeChild/replaceChild/
//! remove/contains/compareDocumentPosition/normalize/cloneNode), and
//! `child_node_variadic.rs` (the `ParentNode`/`ChildNode` variadic
//! helpers) — this file keeps the shared `is_ancestor`/`next_sibling_of`/
//! `first_child_of` helpers, `valid_tag_name`, and the single public entry
//! point, `define_mutation_methods`.

use std::ffi::CString;

use quickjs_sys as sys;

use super::attribute_methods::{
    node_get_attribute, node_get_attribute_names, node_has_attribute, node_remove_attribute,
    node_set_attribute,
};
use super::child_node_variadic::{
    node_after, node_append, node_before, node_prepend, node_replace_with,
};
use super::content::insert_adjacent_html;
use super::tree_edit::{
    node_append_child, node_clone_node, node_compare_document_position, node_contains,
    node_insert_before, node_normalize, node_remove, node_remove_child, node_replace_child,
};

const MAX_TAG_NAME_LENGTH: usize = 64;

pub(super) fn valid_tag_name(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= MAX_TAG_NAME_LENGTH
        && tag
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

pub(super) fn is_ancestor(dom: &dom::Dom, ancestor: dom::NodeId, mut node: dom::NodeId) -> bool {
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
