//! The `Event` native class itself: opaque `EventState`, its property
//! getters/`preventDefault`/`stopPropagation`, the JS `Event` constructor,
//! `register`, and the [`make_event`]/[`create_event`] builders every
//! dispatch path in [`super::dispatch`] uses to construct one.

use quickjs_sys as sys;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use super::util::{getter, read_string, string, type_error, Getter};

pub(super) const EVENT_CLASS_KIND: &str = "Event";

pub(super) struct EventState {
    pub(super) event_type: String,
    pub(super) target: Option<dom::NodeId>,
    pub(super) current_target: Option<dom::NodeId>,
    pub(super) bubbles: bool,
    pub(super) cancelable: bool,
    pub(super) default_prevented: bool,
    pub(super) propagation_stopped: bool,
    /// Set only while a `passive: true` listener's callback is on the stack
    /// (see `call_record`) — real passive-listener semantics: calling
    /// `preventDefault()` from inside one is silently ignored rather than
    /// actually canceling the event, regardless of `cancelable`.
    pub(super) in_passive_listener: bool,
}

pub(super) unsafe fn state(ctx: *mut sys::JSContext, value: sys::JSValue) -> *mut EventState {
    sys::JS_GetOpaque(
        value,
        crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), EVENT_CLASS_KIND),
    ) as *mut EventState
}

unsafe extern "C" fn finalizer(rt: *mut sys::JSRuntime, value: sys::JSValue) {
    let p = sys::JS_GetOpaque(
        value,
        crate::class_registry::class_id_for(rt, EVENT_CLASS_KIND),
    ) as *mut EventState;
    if !p.is_null() {
        drop(Box::from_raw(p));
    }
}

unsafe extern "C" fn event_type(ctx: *mut sys::JSContext, value: sys::JSValue) -> sys::JSValue {
    let p = state(ctx, value);
    if p.is_null() {
        sys::js_undefined()
    } else {
        string(ctx, &(*p).event_type)
    }
}

unsafe fn event_node(ctx: *mut sys::JSContext, value: sys::JSValue, current: bool) -> sys::JSValue {
    let p = state(ctx, value);
    if p.is_null() {
        return sys::js_null();
    }
    let id = if current {
        (*p).current_target
    } else {
        (*p).target
    };
    id.map(|id| {
        crate::dom_bindings::node_object(
            ctx,
            crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), "Node"),
            id,
        )
    })
    .unwrap_or_else(sys::js_null)
}

unsafe extern "C" fn target(ctx: *mut sys::JSContext, value: sys::JSValue) -> sys::JSValue {
    event_node(ctx, value, false)
}

unsafe extern "C" fn current_target(ctx: *mut sys::JSContext, value: sys::JSValue) -> sys::JSValue {
    event_node(ctx, value, true)
}

unsafe extern "C" fn bubbles(ctx: *mut sys::JSContext, value: sys::JSValue) -> sys::JSValue {
    let p = state(ctx, value);
    sys::js_bool(!p.is_null() && (*p).bubbles)
}

unsafe extern "C" fn cancelable(ctx: *mut sys::JSContext, value: sys::JSValue) -> sys::JSValue {
    let p = state(ctx, value);
    sys::js_bool(!p.is_null() && (*p).cancelable)
}

unsafe extern "C" fn default_prevented(
    ctx: *mut sys::JSContext,
    value: sys::JSValue,
) -> sys::JSValue {
    let p = state(ctx, value);
    sys::js_bool(!p.is_null() && (*p).default_prevented)
}

pub(super) unsafe extern "C" fn prevent_default(
    ctx: *mut sys::JSContext,
    value: sys::JSValue,
    _: c_int,
    _: *mut sys::JSValue,
) -> sys::JSValue {
    let p = state(ctx, value);
    if !p.is_null() && (*p).cancelable && !(*p).in_passive_listener {
        (*p).default_prevented = true
    }
    sys::js_undefined()
}

pub(super) unsafe extern "C" fn stop_propagation(
    ctx: *mut sys::JSContext,
    value: sys::JSValue,
    _: c_int,
    _: *mut sys::JSValue,
) -> sys::JSValue {
    let p = state(ctx, value);
    if !p.is_null() {
        (*p).propagation_stopped = true
    }
    sys::js_undefined()
}

unsafe extern "C" fn event_constructor(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return type_error(ctx, "event type is required");
    }
    let Some(kind) = read_string(ctx, *argv) else {
        return type_error(ctx, "event type must be a string");
    };
    let mut bubbles = false;
    let mut cancelable = false;
    if argc >= 2 {
        let options = *argv.add(1);
        for (name, target) in [("bubbles", &mut bubbles), ("cancelable", &mut cancelable)] {
            let name = CString::new(name).unwrap();
            let value = sys::JS_GetPropertyStr(ctx, options, name.as_ptr());
            if sys::js_is_exception(&value) {
                return value;
            }
            let bool_value = sys::JS_ToBool(ctx, value);
            sys::JS_FreeValue(ctx, value);
            if bool_value < 0 {
                return sys::js_exception();
            }
            *target = bool_value != 0;
        }
    }
    make_event(ctx, None, &kind, bubbles, cancelable)
}

pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("Event").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let id = crate::class_registry::ensure_class(rt, EVENT_CLASS_KIND, &def);
    let proto = sys::JS_NewObject(ctx);
    for (name, func) in [
        ("type", event_type as Getter),
        ("target", target as Getter),
        ("currentTarget", current_target as Getter),
        ("bubbles", bubbles as Getter),
        ("cancelable", cancelable as Getter),
        ("defaultPrevented", default_prevented as Getter),
    ] {
        getter(ctx, proto, name, func)
    }
    for (name, func) in [
        ("preventDefault", prevent_default as sys::JSCFunction),
        ("stopPropagation", stop_propagation as sys::JSCFunction),
    ] {
        let name = CString::new(name).unwrap();
        sys::JS_SetPropertyStr(
            ctx,
            proto,
            name.as_ptr(),
            sys::JS_NewCFunction2(ctx, func, name.as_ptr(), 0, sys::JS_CFUNC_GENERIC, 0),
        );
    }
    // `JS_SetClassProto` only wires `proto` in as the *internal* per-context
    // default for `JS_NewObjectClass`-created instances — it consumes its
    // own reference and has no effect on what JS code reading
    // `Event.prototype` sees. Real JS constructors need that as an
    // explicit, JS-visible `prototype` own property on the constructor
    // function too (needed for `instanceof Event` and for
    // `event_subclasses.rs`'s `CustomEvent`/`KeyboardEvent`/`PointerEvent`
    // prototypes to chain onto the real thing instead of `undefined`), so a
    // duped reference is kept aside for that before handing the original to
    // `JS_SetClassProto`.
    let proto_for_property = sys::JS_DupValue(ctx, proto);
    sys::JS_SetClassProto(ctx, id, proto);
    let name = CString::new("Event").unwrap();
    let constructor = sys::JS_NewCFunction2(
        ctx,
        event_constructor,
        name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetConstructorBit(ctx, constructor, true);
    let proto_name = CString::new("prototype").unwrap();
    sys::JS_SetPropertyStr(ctx, constructor, proto_name.as_ptr(), proto_for_property);
    let global = sys::JS_GetGlobalObject(ctx);
    sys::JS_SetPropertyStr(ctx, global, name.as_ptr(), constructor);
    sys::JS_FreeValue(ctx, global);
}

/// Builds an undispatched `Event`-class object with no target yet (`target`/
/// `currentTarget` read as `null` until a real `dispatchEvent` call fills
/// them in) — the seam `event_subclasses.rs` builds `CustomEvent`/
/// `KeyboardEvent`/`PointerEvent` on top of, so those constructors don't
/// need to know how `EventState`/the `Event` native class are represented.
pub(crate) unsafe fn create_event(
    ctx: *mut sys::JSContext,
    kind: &str,
    bubbles: bool,
    cancelable: bool,
) -> sys::JSValue {
    make_event(ctx, None, kind, bubbles, cancelable)
}

pub(super) unsafe fn make_event(
    ctx: *mut sys::JSContext,
    target: Option<dom::NodeId>,
    kind: &str,
    bubbles: bool,
    cancelable: bool,
) -> sys::JSValue {
    let e = sys::JS_NewObjectClass(
        ctx,
        crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), EVENT_CLASS_KIND),
    );
    if sys::js_is_exception(&e) {
        return e;
    }
    let s = EventState {
        event_type: kind.into(),
        target,
        current_target: None,
        bubbles,
        cancelable,
        default_prevented: false,
        propagation_stopped: false,
        in_passive_listener: false,
    };
    sys::JS_SetOpaque(e, Box::into_raw(Box::new(s)) as *mut c_void);
    e
}
