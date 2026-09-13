//! The three-phase (capture/target/bubble) DOM dispatch walk, the
//! nested-dispatch depth guard, and the `dispatchEvent`/`EventTarget`
//! surface (both the real `Node`-tree-walking form and the single-target-
//! phase form used by non-`Node` targets like `window`/`document`).
//!
//! Split into `guard.rs` (the nested-dispatch depth guard), `phase.rs`
//! (`Phase` and per-node listener-snapshot dispatch), and `chain.rs`
//! (ancestor-chain building and the three-phase traversal) — this file
//! keeps `dispatch`/`dispatch_existing`/`dispatch_event` and the
//! `EventTarget` surface itself.

use quickjs_sys as sys;
use std::ffi::CString;
use std::os::raw::c_int;

use crate::events::event_class::{create_event, make_event, state};
use crate::events::listeners::{add, remove};
use crate::events::util::{read_string, type_error};

mod chain;
mod guard;
mod phase;

use chain::{build_ancestor_chain, run_phases};
use guard::begin_dispatch;
use phase::{dispatch_at, Phase};

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
    // Real default action (see `dom_bindings::run_default_click_action`'s
    // own doc) - runs only when nothing canceled the event, same
    // "preventDefault() suppresses the browser's own subsequent action"
    // real semantics `canceled` already gates everything else on.
    if !canceled && kind == "click" {
        crate::dom_bindings::run_default_click_action(ctx, node);
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
    let event_state = state(ctx, event);
    let canceled = (*event_state).default_prevented;
    // Real default action - see `dispatch`'s own identical hook for why.
    if !canceled && (*event_state).event_type == "click" {
        crate::dom_bindings::run_default_click_action(ctx, node);
    }
    !canceled
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
