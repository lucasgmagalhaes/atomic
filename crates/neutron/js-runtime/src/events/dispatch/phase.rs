//! `Phase`, listener invocation, and per-node listener-snapshot dispatch —
//! split out from `dispatch.rs`.

use std::ffi::CString;

use quickjs_sys as sys;

use crate::events::event_class::state;
use crate::events::listeners::{listeners, record_bool, record_callback, remove_matching_record};

/// Which listeners `dispatch_at` should run at this node. Real DOM
/// semantics: at the target itself every listener fires regardless of its
/// `capture` flag, in registration order; on an ancestor, only listeners
/// matching the current traversal direction fire.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Phase {
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

pub(super) unsafe fn dispatch_at(
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
