//! The three-phase (capture/target/bubble) DOM dispatch walk, the
//! nested-dispatch depth guard, and the `dispatchEvent`/`EventTarget`
//! surface (both the real `Node`-tree-walking form and the single-target-
//! phase form used by non-`Node` targets like `window`/`document`).

use quickjs_sys as sys;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

use super::event_class::{create_event, make_event, state};
use super::listeners::{
    add, listeners, record_bool, record_callback, remove, remove_matching_record,
};
use super::util::{read_string, type_error};

const MAX_BUBBLE_DEPTH: usize = 128;
const MAX_NESTED_DISPATCH: usize = 32;

thread_local! {
    static DISPATCH_DEPTHS: RefCell<HashMap<usize, usize>> = RefCell::new(HashMap::new());
}

struct DispatchGuard {
    ctx: usize,
}

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        DISPATCH_DEPTHS.with(|depths| {
            let mut depths = depths.borrow_mut();
            let depth = depths
                .get_mut(&self.ctx)
                .expect("dispatch depth must exist");
            *depth -= 1;
            if *depth == 0 {
                depths.remove(&self.ctx);
            }
        });
    }
}

fn begin_dispatch(ctx: *mut sys::JSContext) -> Option<DispatchGuard> {
    let ctx = ctx as usize;
    DISPATCH_DEPTHS.with(|depths| {
        let mut depths = depths.borrow_mut();
        let depth = depths.entry(ctx).or_default();
        if *depth >= MAX_NESTED_DISPATCH {
            return None;
        }
        *depth += 1;
        Some(DispatchGuard { ctx })
    })
}

/// Which listeners `dispatch_at` should run at this node. Real DOM
/// semantics: at the target itself every listener fires regardless of its
/// `capture` flag, in registration order; on an ancestor, only listeners
/// matching the current traversal direction fire.
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Capture,
    Target,
    Bubble,
}

fn record_matches_phase(phase: Phase, capture: bool) -> bool {
    match phase {
        Phase::Target => true,
        Phase::Capture => capture,
        Phase::Bubble => !capture,
    }
}

/// Invokes one listener record's callback — a plain function is called with
/// `node` as `this` (existing behavior); a `handleEvent`-style listener
/// object is called with itself as `this`, per spec, and silently does
/// nothing if `handleEvent` turned out not to be callable by the time of
/// the call (`is_valid_listener` already checked at registration time, but
/// nothing stops a script from replacing it afterward) — documented
/// simplification rather than a thrown error, since real browsers also
/// just skip a non-callable `handleEvent` rather than treating it as fatal
/// to the whole dispatch.
unsafe fn call_record(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    event: sys::JSValue,
    callback: sys::JSValue,
) -> sys::JSValue {
    if sys::JS_IsFunction(ctx, callback) {
        let mut args = [sys::JS_DupValue(ctx, event)];
        let r = sys::JS_Call(ctx, callback, node, 1, args.as_mut_ptr());
        sys::JS_FreeValue(ctx, args[0]);
        r
    } else {
        let name = CString::new("handleEvent").unwrap();
        let handler = sys::JS_GetPropertyStr(ctx, callback, name.as_ptr());
        if !sys::JS_IsFunction(ctx, handler) {
            sys::JS_FreeValue(ctx, handler);
            return sys::js_undefined();
        }
        let mut args = [sys::JS_DupValue(ctx, event)];
        let r = sys::JS_Call(ctx, handler, callback, 1, args.as_mut_ptr());
        sys::JS_FreeValue(ctx, args[0]);
        sys::JS_FreeValue(ctx, handler);
        r
    }
}

unsafe fn dispatch_at(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    event: sys::JSValue,
    phase: Phase,
    first_exception: &mut Option<sys::JSValue>,
) {
    let p = state(ctx, event);
    let event_type = (*p).event_type.clone();
    let all = listeners(ctx, node);
    let name = CString::new(event_type.as_str()).unwrap_or_default();
    let list = sys::JS_GetPropertyStr(ctx, all, name.as_ptr());
    sys::JS_FreeValue(ctx, all);
    if !sys::JS_IsArray(list) {
        sys::JS_FreeValue(ctx, list);
        return;
    }
    let mut len = 0;
    sys::JS_GetLength(ctx, list, &mut len);
    // Snapshot (callback, capture, once, passive) up front: invoking one
    // listener can register/remove others via addEventListener/
    // removeEventListener, and real dispatch semantics run exactly the
    // listeners that existed when this phase's traversal of this node
    // began, not whatever the list mutates into mid-iteration.
    struct Snapshot {
        callback: sys::JSValue,
        capture: bool,
        once: bool,
        passive: bool,
    }
    let mut snapshot = Vec::new();
    for i in 0..len as u32 {
        let record = sys::JS_GetPropertyUint32(ctx, list, i);
        if record.tag == sys::JS_TAG_UNDEFINED {
            sys::JS_FreeValue(ctx, record);
            continue;
        }
        let capture = record_bool(ctx, record, b"capture\0");
        if !record_matches_phase(phase, capture) {
            sys::JS_FreeValue(ctx, record);
            continue;
        }
        snapshot.push(Snapshot {
            callback: record_callback(ctx, record),
            capture,
            once: record_bool(ctx, record, b"once\0"),
            passive: record_bool(ctx, record, b"passive\0"),
        });
        sys::JS_FreeValue(ctx, record);
    }
    sys::JS_FreeValue(ctx, list);
    for entry in snapshot {
        if entry.passive {
            (*p).in_passive_listener = true;
        }
        let r = call_record(ctx, node, event, entry.callback);
        (*p).in_passive_listener = false;
        if sys::js_is_exception(&r) {
            let exception = sys::JS_GetException(ctx);
            if first_exception.is_none() {
                *first_exception = Some(exception);
            } else {
                sys::JS_FreeValue(ctx, exception);
            }
        }
        sys::JS_FreeValue(ctx, r);
        if entry.once {
            remove_matching_record(ctx, node, &event_type, entry.callback, entry.capture);
        }
        sys::JS_FreeValue(ctx, entry.callback);
    }
}

/// Builds the ancestor chain from `target_id` up to (and bounded by)
/// `MAX_BUBBLE_DEPTH` nodes — shared by `dispatch`/`dispatch_existing` so
/// both run the same three-phase (capture down, target, bubble up)
/// traversal instead of duplicating it. `None` means the tree was too deep
/// to safely traverse (matches the previous single-pass loop's own bound
/// exactly: up to `MAX_BUBBLE_DEPTH` nodes are collected, and if a node
/// beyond that still has a parent pending, this reports "too deep" the same
/// way the old code did after its own bounded loop).
unsafe fn build_ancestor_chain(
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
unsafe fn run_phases(
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

pub(crate) unsafe fn dispatch(ctx: *mut sys::JSContext, node: sys::JSValue, kind: &str) -> bool {
    let Some(_dispatch_guard) = begin_dispatch(ctx) else {
        type_error(ctx, "nested event dispatch limit exceeded");
        return false;
    };
    let Some(target_id) = crate::dom_bindings::node_id(ctx, node) else {
        return true;
    };
    let event = make_event(ctx, Some(target_id), kind, true, true);
    if sys::js_is_exception(&event) {
        return false;
    }
    let Some(chain) = build_ancestor_chain(ctx, target_id) else {
        sys::JS_FreeValue(ctx, event);
        type_error(ctx, "event propagation limit exceeded");
        return false;
    };
    let mut first_exception = None;
    run_phases(ctx, &chain, event, &mut first_exception);
    let canceled = (*state(ctx, event)).default_prevented;
    sys::JS_FreeValue(ctx, event);
    if let Some(exception) = first_exception {
        sys::JS_Throw(ctx, exception);
        return false;
    }
    !canceled
}

unsafe fn dispatch_existing(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    event: sys::JSValue,
) -> bool {
    let Some(_dispatch_guard) = begin_dispatch(ctx) else {
        type_error(ctx, "nested event dispatch limit exceeded");
        return false;
    };
    let Some(target_id) = crate::dom_bindings::node_id(ctx, node) else {
        return true;
    };
    let event_state = state(ctx, event);
    if event_state.is_null() {
        type_error(ctx, "dispatchEvent requires an Event");
        return false;
    }
    (*event_state).target = Some(target_id);
    (*event_state).current_target = None;
    (*event_state).propagation_stopped = false;
    let Some(chain) = build_ancestor_chain(ctx, target_id) else {
        type_error(ctx, "event propagation limit exceeded");
        return false;
    };
    let mut first_exception = None;
    run_phases(ctx, &chain, event, &mut first_exception);
    if let Some(exception) = first_exception {
        sys::JS_Throw(ctx, exception);
        return false;
    }
    !(*state(ctx, event)).default_prevented
}

unsafe extern "C" fn dispatch_event(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_bool(true);
    }
    let supplied = *argv;
    let result = if !state(ctx, supplied).is_null() {
        dispatch_existing(ctx, node, supplied)
    } else {
        let Some(kind) = read_string(ctx, supplied) else {
            return type_error(ctx, "event type must be a string or Event");
        };
        dispatch(ctx, node, &kind)
    };
    if sys::JS_HasException(ctx) {
        sys::js_exception()
    } else {
        sys::js_bool(result)
    }
}

/// Dispatches an already-built `Event`-shaped `event` value directly at
/// `target` — a single "target phase" call, no capture/bubble DOM-tree walk
/// (there is no `dom::NodeId` to walk a tree from for a target like
/// `window`/`document`/`history`, which are plain JS objects, not `Node`
/// instances). Consumes `event` (frees it before returning) — a caller that
/// needs to set extra properties on it first (e.g. `history.rs`'s
/// `popstate` setting `.state`) builds the event itself via [`create_event`]
/// and calls this afterward, rather than going through [`dispatch_simple`].
pub(crate) unsafe fn dispatch_event_object(
    ctx: *mut sys::JSContext,
    target: sys::JSValue,
    event: sys::JSValue,
) -> bool {
    if sys::js_is_exception(&event) {
        return false;
    }
    let Some(_dispatch_guard) = begin_dispatch(ctx) else {
        sys::JS_FreeValue(ctx, event);
        type_error(ctx, "nested event dispatch limit exceeded");
        return false;
    };
    let mut first_exception = None;
    dispatch_at(ctx, target, event, Phase::Target, &mut first_exception);
    let canceled = (*state(ctx, event)).default_prevented;
    sys::JS_FreeValue(ctx, event);
    if let Some(exception) = first_exception {
        sys::JS_Throw(ctx, exception);
        return false;
    }
    !canceled
}

/// Builds a plain `Event(kind, {bubbles, cancelable})` and dispatches it at
/// `target` via [`dispatch_event_object`] — the common case (lifecycle
/// events like `DOMContentLoaded`/`load`) that needs no extra properties on
/// the event first.
pub(crate) unsafe fn dispatch_simple(
    ctx: *mut sys::JSContext,
    target: sys::JSValue,
    kind: &str,
    bubbles: bool,
    cancelable: bool,
) -> bool {
    let event = create_event(ctx, kind, bubbles, cancelable);
    dispatch_event_object(ctx, target, event)
}

unsafe fn define_event_target_with(
    ctx: *mut sys::JSContext,
    proto: sys::JSValue,
    dispatch_fn: sys::JSCFunction,
) {
    for (name, func, arity) in [
        ("addEventListener", add as sys::JSCFunction, 2),
        ("removeEventListener", remove as sys::JSCFunction, 1),
        ("dispatchEvent", dispatch_fn, 1),
    ] {
        let name = CString::new(name).unwrap();
        sys::JS_SetPropertyStr(
            ctx,
            proto,
            name.as_ptr(),
            sys::JS_NewCFunction2(ctx, func, name.as_ptr(), arity, sys::JS_CFUNC_GENERIC, 0),
        );
    }
}

/// `addEventListener`/`removeEventListener`/`dispatchEvent` for a real
/// `dom::NodeId`-backed `Node` — `dispatchEvent` walks the DOM tree
/// (capture/target/bubble) via [`dispatch`]/[`dispatch_existing`].
pub(crate) unsafe fn define_event_target(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    define_event_target_with(ctx, proto, dispatch_event as sys::JSCFunction);
}

/// Same JS-visible surface as [`define_event_target`], for a target that
/// isn't a `Node` at all (`window`/`document`/`history` are plain JS
/// objects) — `dispatchEvent` here only ever runs a single target-phase
/// call via [`dispatch_simple`]/[`dispatch_event_object`], since there's no
/// `dom::NodeId` to walk a capture/bubble chain from. `addEventListener`/
/// `removeEventListener` are unchanged (`add`/`remove` already operate on
/// whatever JS object they're called on, Node or not).
pub(crate) unsafe fn define_simple_event_target(ctx: *mut sys::JSContext, target: sys::JSValue) {
    define_event_target_with(ctx, target, dispatch_event_simple as sys::JSCFunction);
}

unsafe extern "C" fn dispatch_event_simple(
    ctx: *mut sys::JSContext,
    target: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_bool(true);
    }
    let supplied = *argv;
    let result = if !state(ctx, supplied).is_null() {
        dispatch_event_object(ctx, target, sys::JS_DupValue(ctx, supplied))
    } else {
        let Some(kind) = read_string(ctx, supplied) else {
            return type_error(ctx, "event type must be a string or Event");
        };
        dispatch_simple(ctx, target, &kind, true, true)
    };
    if sys::JS_HasException(ctx) {
        sys::js_exception()
    } else {
        sys::js_bool(result)
    }
}
