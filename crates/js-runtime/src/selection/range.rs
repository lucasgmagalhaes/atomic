//! `Range` (`ROADMAP.md` item 20, "text selection") — real for text-node
//! boundary points only. See `selection/mod.rs`'s own module doc for the
//! full scope note.

use std::os::raw::c_int;

use quickjs_sys as sys;

use super::helpers::{
    get_prop, new_js_string, resolve_prototype, set_prop, AND_END_NODE, AND_END_OFFSET,
    AND_START_NODE, AND_START_OFFSET,
};

unsafe fn read_offset(val: sys::JSValue) -> usize {
    match val.tag {
        sys::JS_TAG_INT => (val.u.int32.max(0)) as usize,
        sys::JS_TAG_FLOAT64 => (val.u.float64.max(0.0)) as usize,
        _ => 0,
    }
}

/// Real only for a `node` that's a live text node - anything else is a
/// documented no-op (real spec throws `InvalidNodeTypeError`; this crate's
/// own convention elsewhere, e.g. `crypto.getRandomValues`'s non-
/// `Uint8Array` case, is to no-op on out-of-scope input rather than build
/// a full spec-shaped exception surface for it).
unsafe fn is_text_node(ctx: *mut sys::JSContext, node: sys::JSValue) -> bool {
    let Some(id) = crate::dom_bindings::node_id(ctx, node) else {
        return false;
    };
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return false;
    }
    matches!(
        (*state).dom.get(id).map(|n| &n.data),
        Some(dom::NodeData::Text(_))
    )
}

unsafe fn set_boundary(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    node_key: &[u8],
    offset_key: &[u8],
    node: sys::JSValue,
    offset: usize,
) {
    if !is_text_node(ctx, node) {
        return;
    }
    set_prop(ctx, this_val, node_key, sys::JS_DupValue(ctx, node));
    set_prop(ctx, this_val, offset_key, sys::js_float64(offset as f64));
}

/// Builds a real collapsed `Range` at `(node, offset)` - used by
/// `Selection::collapse` (real user-driven selection isn't modeled, but a
/// script calling `selection.collapse(node, offset)` gets a real range
/// back). No-op (returns a `Range` still collapsed at the document root)
/// if `node` isn't a real text node - same scope cut every other
/// boundary-setting entry point here already takes.
pub(super) unsafe fn make_collapsed_range_at(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    offset: usize,
) -> sys::JSValue {
    let range = make_range(ctx, sys::js_undefined());
    set_boundary(ctx, range, AND_START_NODE, AND_START_OFFSET, node, offset);
    set_boundary(ctx, range, AND_END_NODE, AND_END_OFFSET, node, offset);
    range
}

pub(super) unsafe extern "C" fn range_constructor(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    make_range(ctx, new_target)
}

/// Builds a fresh `Range`, collapsed at the document's own root (`(root,
/// 0)`/`(root, 0)`) — matches a real freshly-created `Range`. Used by both
/// `new Range()` and `document.createRange()`.
pub(super) unsafe fn make_range(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
) -> sys::JSValue {
    let proto = resolve_prototype(ctx, new_target, "Range");
    let obj = sys::JS_NewObject(ctx);
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);

    let state = crate::host_state::get(ctx);
    if !state.is_null() {
        let root_id = (*state).dom.root();
        let class_id = crate::dom_bindings::node_class_id_for(ctx, &mut (*state).dom, root_id);
        let root_obj = crate::dom_bindings::node_object(ctx, class_id, root_id);
        set_prop(ctx, obj, AND_START_NODE, sys::JS_DupValue(ctx, root_obj));
        set_prop(ctx, obj, AND_END_NODE, root_obj);
        set_prop(ctx, obj, AND_START_OFFSET, sys::js_float64(0.0));
        set_prop(ctx, obj, AND_END_OFFSET, sys::js_float64(0.0));
    }
    obj
}

pub(super) unsafe extern "C" fn range_set_start(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc >= 2 {
        let offset = read_offset(*argv.add(1));
        set_boundary(
            ctx,
            this_val,
            AND_START_NODE,
            AND_START_OFFSET,
            *argv,
            offset,
        );
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn range_set_end(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc >= 2 {
        let offset = read_offset(*argv.add(1));
        set_boundary(ctx, this_val, AND_END_NODE, AND_END_OFFSET, *argv, offset);
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn range_select_node_contents(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc >= 1 {
        let node = *argv;
        if is_text_node(ctx, node) {
            let text_len = crate::dom_bindings::node_id(ctx, node)
                .and_then(|id| {
                    let state = crate::host_state::get(ctx);
                    if state.is_null() {
                        return None;
                    }
                    match (*state).dom.get(id).map(|n| &n.data) {
                        Some(dom::NodeData::Text(t)) => Some(t.chars().count()),
                        _ => None,
                    }
                })
                .unwrap_or(0);
            set_boundary(ctx, this_val, AND_START_NODE, AND_START_OFFSET, node, 0);
            set_boundary(ctx, this_val, AND_END_NODE, AND_END_OFFSET, node, text_len);
        }
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn range_collapse(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let to_start = if argc >= 1 {
        sys::JS_ToBool(ctx, *argv) != 0
    } else {
        true
    };
    if to_start {
        let node = get_prop(ctx, this_val, AND_START_NODE);
        let offset = get_prop(ctx, this_val, AND_START_OFFSET);
        set_prop(ctx, this_val, AND_END_NODE, node);
        set_prop(ctx, this_val, AND_END_OFFSET, offset);
    } else {
        let node = get_prop(ctx, this_val, AND_END_NODE);
        let offset = get_prop(ctx, this_val, AND_END_OFFSET);
        set_prop(ctx, this_val, AND_START_NODE, node);
        set_prop(ctx, this_val, AND_START_OFFSET, offset);
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn range_start_container_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, AND_START_NODE)
}
pub(super) unsafe extern "C" fn range_end_container_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, AND_END_NODE)
}
pub(super) unsafe extern "C" fn range_start_offset_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, AND_START_OFFSET)
}
pub(super) unsafe extern "C" fn range_end_offset_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, AND_END_OFFSET)
}

pub(super) unsafe extern "C" fn range_collapsed_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let start_node = get_prop(ctx, this_val, AND_START_NODE);
    let end_node = get_prop(ctx, this_val, AND_END_NODE);
    let same_node = crate::dom_bindings::node_id(ctx, start_node)
        == crate::dom_bindings::node_id(ctx, end_node);
    sys::JS_FreeValue(ctx, start_node);
    sys::JS_FreeValue(ctx, end_node);
    if !same_node {
        return sys::js_bool(false);
    }
    let start_offset = get_prop(ctx, this_val, AND_START_OFFSET);
    let end_offset = get_prop(ctx, this_val, AND_END_OFFSET);
    let equal = read_offset(start_offset) == read_offset(end_offset);
    sys::js_bool(equal)
}

/// Real cross-node concatenation: a pre-order walk of every real
/// `NodeData::Text` node in the whole document, sliced at the range's two
/// boundary points — see `selection/mod.rs`'s own doc for the full
/// algorithm description.
pub(super) unsafe extern "C" fn range_to_string(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let start_node = get_prop(ctx, this_val, AND_START_NODE);
    let end_node = get_prop(ctx, this_val, AND_END_NODE);
    let start_id = crate::dom_bindings::node_id(ctx, start_node);
    let end_id = crate::dom_bindings::node_id(ctx, end_node);
    sys::JS_FreeValue(ctx, start_node);
    sys::JS_FreeValue(ctx, end_node);
    let start_offset = read_offset(get_prop(ctx, this_val, AND_START_OFFSET));
    let end_offset = read_offset(get_prop(ctx, this_val, AND_END_OFFSET));

    let (Some(start_id), Some(end_id)) = (start_id, end_id) else {
        return new_js_string(ctx, "");
    };

    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return new_js_string(ctx, "");
    }

    let mut text_nodes = Vec::new();
    collect_text_nodes(&(*state).dom, (*state).dom.root(), &mut text_nodes);

    let mut result = String::new();
    let mut in_range = false;
    for (id, text) in &text_nodes {
        let chars: Vec<char> = text.chars().collect();
        if *id == start_id {
            in_range = true;
            let from = start_offset.min(chars.len());
            let to = if *id == end_id {
                end_offset.min(chars.len())
            } else {
                chars.len()
            };
            if from < to {
                result.extend(&chars[from..to]);
            }
            if *id == end_id {
                break;
            }
            continue;
        }
        if in_range {
            if *id == end_id {
                let to = end_offset.min(chars.len());
                result.extend(&chars[..to]);
                break;
            } else {
                result.push_str(text);
            }
        }
    }
    new_js_string(ctx, &result)
}

fn collect_text_nodes(dom: &dom::Dom, id: dom::NodeId, out: &mut Vec<(dom::NodeId, String)>) {
    let Some(node) = dom.get(id) else { return };
    if let dom::NodeData::Text(text) = &node.data {
        out.push((id, text.clone()));
    }
    for &child in &node.children {
        collect_text_nodes(dom, child, out);
    }
}
