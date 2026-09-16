//! Ancestor-chain building and the three-phase traversal over it — split
//! out from `dispatch.rs`.

use quickjs_sys as sys;

use crate::events::event_class::state;

use super::phase::{dispatch_at, Phase};

const MAX_BUBBLE_DEPTH: usize = 128;

/// Builds the ancestor chain from `target_id` up to (and bounded by)
/// `MAX_BUBBLE_DEPTH` nodes — shared by `dispatch`/`dispatch_existing` so
/// both run the same three-phase (capture down, target, bubble up)
/// traversal instead of duplicating it. `None` means the tree was too deep
/// to safely traverse (matches the previous single-pass loop's own bound
/// exactly: up to `MAX_BUBBLE_DEPTH` nodes are collected, and if a node
/// beyond that still has a parent pending, this reports "too deep" the same
/// way the old code did after its own bounded loop).
pub(super) unsafe fn build_ancestor_chain(
    ctx: *mut sys::JSContext,
    target_id: dom::NodeId,
) -> Option<Vec<dom::NodeId>> {
    let mut chain = Vec::new();
    let mut current = Some(target_id);
    for _ in 0..MAX_BUBBLE_DEPTH {
        let Some(id) = current else { break };
        chain.push(id);
        current = crate::dom_bindings::parent_node_id(ctx, id);
    }
    if current.is_some() {
        None
    } else {
        Some(chain)
    }
}

/// Runs the real three-phase traversal (capturing root->target, at-target,
/// then bubbling target->root if `event.bubbles`) over `chain` (as built by
/// [`build_ancestor_chain`], `chain[0]` being the target), stopping early
/// wherever `stopPropagation()` was called — matches `Event`'s own
/// per-node-then-check-before-next-node semantics: every listener at the
/// current node always finishes running before propagation is checked, so
/// `stopPropagation()` never cuts off other listeners on the same node,
/// only movement to the next one.
pub(super) unsafe fn run_phases(
    ctx: *mut sys::JSContext,
    chain: &[dom::NodeId],
    event: sys::JSValue,
    first_exception: &mut Option<sys::JSValue>,
) {
    // Per-node, not a single fixed "Node" class id: `node_object`'s cache
    // is keyed by `NodeId` alone (one JS object per node, regardless of
    // which class it was built with — see its own doc), so building a
    // dispatch-time wrapper with the generic `Node` class instead of the
    // node's real subclass (`HTMLFormElement`, `HTMLInputElement`, ...)
    // would silently overwrite that node's cached identity with one
    // missing every subclass-specific method (`form.reset()`,
    // `select.selectedIndex`, ...) the next time anything looks it up —
    // a real bug this fixes, caught by `form_reset_submit_test.rs`: an
    // `"input"` event bubbling from a text field through its `<form>`
    // ancestor used to leave that form's cached object without
    // `reset`/`requestSubmit` for every later access, not just this one.
    (*state(ctx, event)).path = chain.to_vec();
    let host_dom: *mut dom::Dom = {
        let state_ptr = crate::host_state::get(ctx);
        if state_ptr.is_null() {
            std::ptr::null_mut()
        } else {
            &mut (*state_ptr).dom as *mut dom::Dom
        }
    };
    let run_at = |ctx: *mut sys::JSContext,
                  id: dom::NodeId,
                  phase: Phase,
                  first_exception: &mut Option<sys::JSValue>| {
        let p = state(ctx, event);
        (*p).current_target = Some(id);
        let node_class = crate::dom_bindings::node_class_id_for(ctx, host_dom, id);
        let node = crate::dom_bindings::node_object(ctx, node_class, id);
        dispatch_at(ctx, node, event, phase, first_exception);
        sys::JS_FreeValue(ctx, node);
    };
    for &id in chain[1..].iter().rev() {
        if (*state(ctx, event)).propagation_stopped {
            return;
        }
        run_at(ctx, id, Phase::Capture, first_exception);
    }
    if (*state(ctx, event)).propagation_stopped {
        return;
    }
    run_at(ctx, chain[0], Phase::Target, first_exception);
    if !(*state(ctx, event)).bubbles {
        return;
    }
    for &id in &chain[1..] {
        if (*state(ctx, event)).propagation_stopped {
            return;
        }
        run_at(ctx, id, Phase::Bubble, first_exception);
    }
}
