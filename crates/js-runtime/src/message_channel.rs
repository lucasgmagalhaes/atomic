//! `MessageChannel`/`MessagePort` (`ROADMAP.md` items 29/30/31): real
//! same-realm structured-clone delivery between two entangled ports,
//! queued and pumped like every other "no real event loop yet" primitive
//! in this crate (`timers`/`fetch_async`/`mutation_observer`).
//!
//! Structured Clone integration (item 31): `postMessage(data)` clones
//! `data` immediately via [`crate::value_bridge::deep_clone`] (already a
//! real, detached `JSValue` — no shared-reference `JS_DupValue` aliasing),
//! the same primitive `history.pushState`/`replaceState` already uses for
//! `state` and `structuredClone()` exposes directly to script — this is
//! its second real native caller.
//!
//! Real `onmessage` IDL attribute (an implicit, single listener — same
//! delivery semantics `addEventListener("message", ...)` gives, invoked
//! in addition to any explicitly-added listeners) plus full
//! `addEventListener`/`removeEventListener`/`dispatchEvent` via
//! [`crate::events::define_simple_event_target`]. `close()` marks a port
//! dead; a `postMessage` aimed at (or sent from) a closed port is a silent
//! no-op, matching a real closed port dropping messages either direction.
//! `new MessagePort()` throws (matches real browsers — only
//! `MessageChannel` ever produces one, same convention `AbortSignal`'s own
//! constructor already documents).
//!
//! Real Transferable objects now (`ROADMAP.md` item 32), scoped to
//! `Uint8Array` — `postMessage(data, transfer)`'s second argument, if a
//! real array, has each `Uint8Array` element's backing buffer detached
//! (`byteLength` becomes `0`, matching a real transferred buffer) *after*
//! `deep_clone` has already copied its bytes — real "ownership moved"
//! observable behavior, not just accepted-and-ignored. Not modeled: a
//! bare `ArrayBuffer` with no view, and every other `TypedArray` kind —
//! same narrower scope `value_bridge.rs`'s own doc already documents for
//! Transferable objects generally.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

const PORT_ID_PROP: &[u8] = b"__port_id\0";
const ONMESSAGE_PROP: &[u8] = b"__onmessage\0";

struct Port {
    /// This port's own JS object, dup'd — a peer looks this up by id to
    /// dispatch directly onto it.
    object: sys::JSValue,
    peer: u32,
    closed: bool,
}

#[derive(Default)]
struct Registry {
    next_id: u32,
    ports: HashMap<u32, Port>,
    /// `(target port id, already-cloned data)`, queued by `postMessage`
    /// and delivered by [`pump`].
    pending: Vec<(u32, sys::JSValue)>,
}

thread_local! {
    // Keyed by JSContext pointer, same convention every other registry in
    // this crate (`timers`/`mutation_observer`/...) uses.
    static REGISTRIES: RefCell<HashMap<usize, Registry>> = RefCell::new(HashMap::new());
}

unsafe fn get_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &[u8]) -> sys::JSValue {
    sys::JS_GetPropertyStr(ctx, obj, name.as_ptr() as *const _)
}
unsafe fn set_prop(ctx: *mut sys::JSContext, obj: sys::JSValue, name: &[u8], value: sys::JSValue) {
    sys::JS_SetPropertyStr(ctx, obj, name.as_ptr() as *const _, value);
}
unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}
unsafe fn throw_type_error(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_string(ctx, s))
}
unsafe fn port_id(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> Option<u32> {
    let value = get_prop(ctx, this_val, PORT_ID_PROP);
    let id = match value.tag {
        sys::JS_TAG_INT => Some(value.u.int32 as u32),
        sys::JS_TAG_FLOAT64 => Some(value.u.float64 as u32),
        _ => None,
    };
    sys::JS_FreeValue(ctx, value);
    id
}

unsafe fn resolve_prototype(
    ctx: *mut sys::JSContext,
    new_target: sys::JSValue,
    fallback_ctor_name: &str,
) -> sys::JSValue {
    if new_target.tag != sys::JS_TAG_UNDEFINED {
        let proto = get_prop(ctx, new_target, b"prototype\0");
        if proto.tag != sys::JS_TAG_UNDEFINED {
            return proto;
        }
        sys::JS_FreeValue(ctx, proto);
    }
    let global = sys::JS_GetGlobalObject(ctx);
    let ctor_name = CString::new(fallback_ctor_name).unwrap();
    let ctor = sys::JS_GetPropertyStr(ctx, global, ctor_name.as_ptr());
    sys::JS_FreeValue(ctx, global);
    let proto = get_prop(ctx, ctor, b"prototype\0");
    sys::JS_FreeValue(ctx, ctor);
    proto
}

/// Builds one real `MessagePort` object (real prototype resolved from the
/// global `MessagePort` constructor's own `.prototype`) and registers its
/// registry entry. `peer` is filled in by the caller once both ports of a
/// pair exist (a fresh port temporarily points at itself, corrected right
/// after).
unsafe fn make_port(ctx: *mut sys::JSContext) -> (sys::JSValue, u32) {
    let proto = resolve_prototype(ctx, sys::js_undefined(), "MessagePort");
    let obj = sys::JS_NewObject(ctx);
    if proto.tag != sys::JS_TAG_UNDEFINED {
        sys::JS_SetPrototype(ctx, obj, proto);
    }
    sys::JS_FreeValue(ctx, proto);
    crate::events::define_simple_event_target(ctx, obj);

    let id = REGISTRIES.with(|reg| {
        let mut map = reg.borrow_mut();
        let registry = map.entry(ctx as usize).or_default();
        let id = registry.next_id;
        registry.next_id = registry.next_id.wrapping_add(1);
        registry.ports.insert(
            id,
            Port {
                object: sys::JS_DupValue(ctx, obj),
                peer: id,
                closed: false,
            },
        );
        id
    });
    set_prop(ctx, obj, PORT_ID_PROP, sys::js_float64(id as f64));
    (obj, id)
}

unsafe extern "C" fn message_port_constructor(
    ctx: *mut sys::JSContext,
    _new_target: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    throw_type_error(ctx, "Illegal constructor")
}

unsafe extern "C" fn message_channel_constructor(
    ctx: *mut sys::JSContext,
    _new_target: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let (port1, id1) = make_port(ctx);
    let (port2, id2) = make_port(ctx);
    REGISTRIES.with(|reg| {
        let mut map = reg.borrow_mut();
        if let Some(registry) = map.get_mut(&(ctx as usize)) {
            if let Some(p) = registry.ports.get_mut(&id1) {
                p.peer = id2;
            }
            if let Some(p) = registry.ports.get_mut(&id2) {
                p.peer = id1;
            }
        }
    });
    let obj = sys::JS_NewObject(ctx);
    set_prop(ctx, obj, b"port1\0", port1);
    set_prop(ctx, obj, b"port2\0", port2);
    obj
}

unsafe extern "C" fn port_post_message(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    let Some(id) = port_id(ctx, this_val) else {
        return sys::js_undefined();
    };
    let data = if argc >= 1 {
        *argv
    } else {
        sys::js_undefined()
    };
    let cloned = crate::value_bridge::deep_clone(ctx, data);

    // Real transfer (`ROADMAP.md` item 32): detach each transferred
    // `Uint8Array`'s backing buffer only *after* the clone above has
    // already copied its bytes - detaching first would clone empty data.
    if argc >= 2 {
        let transfer = *argv.add(1);
        if sys::JS_IsArray(transfer) {
            let mut len: i64 = 0;
            sys::JS_GetLength(ctx, transfer, &mut len);
            for i in 0..len.max(0) as u32 {
                let item = sys::JS_GetPropertyUint32(ctx, transfer, i);
                let mut size: usize = 0;
                if !sys::JS_GetUint8Array(ctx, &mut size, item).is_null() {
                    let buffer = sys::JS_GetTypedArrayBuffer(
                        ctx,
                        item,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    );
                    if !sys::js_is_exception(&buffer) {
                        sys::JS_DetachArrayBuffer(ctx, buffer);
                        sys::JS_FreeValue(ctx, buffer);
                    }
                } else {
                    // Not a real Uint8Array - clear the stray TypeError
                    // `JS_GetUint8Array` throws for any other value (see
                    // `value_bridge.rs`'s own doc on this exact gotcha),
                    // and silently skip it (narrower scope, not a thrown
                    // `DataCloneError` for every invalid transfer target).
                    let stray = sys::JS_GetException(ctx);
                    sys::JS_FreeValue(ctx, stray);
                }
                sys::JS_FreeValue(ctx, item);
            }
        }
    }

    REGISTRIES.with(|reg| {
        let mut map = reg.borrow_mut();
        let Some(registry) = map.get_mut(&(ctx as usize)) else {
            sys::JS_FreeValue(ctx, cloned);
            return;
        };
        let self_closed = registry.ports.get(&id).map(|p| p.closed).unwrap_or(true);
        let peer = registry.ports.get(&id).map(|p| p.peer);
        let peer_closed = peer
            .and_then(|p| registry.ports.get(&p))
            .map(|p| p.closed)
            .unwrap_or(true);
        if self_closed || peer_closed {
            sys::JS_FreeValue(ctx, cloned);
            return;
        }
        registry.pending.push((peer.unwrap(), cloned));
    });
    sys::js_undefined()
}

unsafe extern "C" fn port_close(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    if let Some(id) = port_id(ctx, this_val) {
        REGISTRIES.with(|reg| {
            if let Some(registry) = reg.borrow_mut().get_mut(&(ctx as usize)) {
                if let Some(port) = registry.ports.get_mut(&id) {
                    port.closed = true;
                }
            }
        });
    }
    sys::js_undefined()
}

unsafe extern "C" fn port_start(
    _ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    // Real ports here are never paused (no explicit queuing-before-start
    // model) - start() exists only so real code calling it doesn't throw.
    sys::js_undefined()
}

unsafe extern "C" fn onmessage_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    get_prop(ctx, this_val, ONMESSAGE_PROP)
}
unsafe extern "C" fn onmessage_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    set_prop(ctx, this_val, ONMESSAGE_PROP, sys::JS_DupValue(ctx, val));
    sys::js_undefined()
}

/// Delivers every queued `postMessage` — one real `"message"` event per
/// pending item, dispatched on the target port's own object (both to its
/// `addEventListener("message", ...)` listeners and, additionally, its
/// `onmessage` IDL attribute if set). Called from
/// [`crate::Context::run_pending_timers`], same cadence
/// `timers`/`mutation_observer` already use. Returns how many messages
/// were delivered.
pub(crate) unsafe fn pump(ctx: *mut sys::JSContext) -> usize {
    let pending = REGISTRIES.with(|reg| {
        reg.borrow_mut()
            .get_mut(&(ctx as usize))
            .map(|r| std::mem::take(&mut r.pending))
            .unwrap_or_default()
    });
    if pending.is_empty() {
        return 0;
    }
    let mut delivered = 0;
    for (target_id, data) in pending {
        let target_object = REGISTRIES.with(|reg| {
            reg.borrow()
                .get(&(ctx as usize))
                .and_then(|r| r.ports.get(&target_id))
                .filter(|p| !p.closed)
                .map(|p| sys::JS_DupValue(ctx, p.object))
        });
        let Some(target_object) = target_object else {
            sys::JS_FreeValue(ctx, data);
            continue;
        };
        let event = crate::events::create_event(ctx, "message", false, false);
        if sys::js_is_exception(&event) {
            sys::JS_FreeValue(ctx, data);
            sys::JS_FreeValue(ctx, target_object);
            continue;
        }
        set_prop(ctx, event, b"data\0", data);
        let event_for_dispatch = sys::JS_DupValue(ctx, event);
        crate::events::dispatch_event_object(ctx, target_object, event_for_dispatch);

        let handler = get_prop(ctx, target_object, ONMESSAGE_PROP);
        if sys::JS_IsFunction(ctx, handler) {
            let mut arg = sys::JS_DupValue(ctx, event);
            let result = sys::JS_Call(ctx, handler, target_object, 1, &mut arg);
            sys::JS_FreeValue(ctx, result);
            sys::JS_FreeValue(ctx, arg);
        }
        sys::JS_FreeValue(ctx, handler);
        sys::JS_FreeValue(ctx, event);
        sys::JS_FreeValue(ctx, target_object);
        delivered += 1;
    }
    delivered
}

/// Frees every registered port's object (and any still-pending cloned
/// data) for `ctx` — must run before `JS_FreeContext`, same ordering
/// requirement every other module's `cleanup(ctx)` already documents.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    if let Some(registry) = REGISTRIES.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
        for (_, port) in registry.ports {
            sys::JS_FreeValue(ctx, port.object);
        }
        for (_, data) in registry.pending {
            sys::JS_FreeValue(ctx, data);
        }
    }
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let port_proto = sys::JS_NewObject(ctx);
    crate::js_helpers::define_method(ctx, port_proto, "postMessage", port_post_message, 1);
    crate::js_helpers::define_method(ctx, port_proto, "close", port_close, 0);
    crate::js_helpers::define_method(ctx, port_proto, "start", port_start, 0);
    crate::js_helpers::define_getter_setter(
        ctx,
        port_proto,
        "onmessage",
        onmessage_get,
        onmessage_set,
    );
    let port_ctor_name = CString::new("MessagePort").unwrap();
    let port_ctor = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<
            unsafe extern "C" fn(
                *mut sys::JSContext,
                sys::JSValue,
                c_int,
                *mut sys::JSValue,
            ) -> sys::JSValue,
            sys::JSCFunction,
        >(message_port_constructor),
        port_ctor_name.as_ptr(),
        0,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(ctx, port_ctor, proto_name.as_ptr(), port_proto);
    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, port_ctor_name.as_ptr(), port_ctor);

    let channel_ctor_name = CString::new("MessageChannel").unwrap();
    let channel_ctor = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<
            unsafe extern "C" fn(
                *mut sys::JSContext,
                sys::JSValue,
                c_int,
                *mut sys::JSValue,
            ) -> sys::JSValue,
            sys::JSCFunction,
        >(message_channel_constructor),
        channel_ctor_name.as_ptr(),
        0,
        sys::JS_CFUNC_CONSTRUCTOR_OR_FUNC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, global, channel_ctor_name.as_ptr(), channel_ctor);
    sys::JS_FreeValue(ctx, global);
}
