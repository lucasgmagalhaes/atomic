//! Bounded DOM-style event dispatch for `Node` objects.
//!
//! Event state is native structured data. User code is invoked only as a
//! QuickJS function; no supplied string is parsed or evaluated by this layer.
use quickjs_sys as sys;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

const LISTENERS_PROP: &[u8] = b"__listeners\0";
const MAX_LISTENERS_PER_TYPE: i64 = 64;
const EVENT_CLASS_KIND: &str = "Event";

struct EventState {
    event_type: String,
    target: dom::NodeId,
    current_target: Option<dom::NodeId>,
    bubbles: bool,
    cancelable: bool,
    default_prevented: bool,
    propagation_stopped: bool,
}
type Getter = unsafe extern "C" fn(*mut sys::JSContext, sys::JSValue) -> sys::JSValue;

unsafe fn string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}
unsafe fn type_error(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, string(ctx, s))
}
unsafe fn read_string(ctx: *mut sys::JSContext, value: sys::JSValue) -> Option<String> {
    let mut len = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, value, false);
    if ptr.is_null() {
        return None;
    };
    let s = String::from_utf8_lossy(std::slice::from_raw_parts(ptr as *const u8, len)).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}
unsafe fn state(ctx: *mut sys::JSContext, value: sys::JSValue) -> *mut EventState {
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
        Some((*p).target)
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
unsafe extern "C" fn prevent_default(
    ctx: *mut sys::JSContext,
    value: sys::JSValue,
    _: c_int,
    _: *mut sys::JSValue,
) -> sys::JSValue {
    let p = state(ctx, value);
    if !p.is_null() && (*p).cancelable {
        (*p).default_prevented = true
    }
    sys::js_undefined()
}
unsafe extern "C" fn stop_propagation(
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

unsafe fn getter(ctx: *mut sys::JSContext, proto: sys::JSValue, name: &str, func: Getter) {
    let name = CString::new(name).unwrap();
    let f = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(func),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        f,
        sys::js_undefined(),
        sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom)
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
    sys::JS_SetClassProto(ctx, id, proto)
}
unsafe fn make_event(ctx: *mut sys::JSContext, target: dom::NodeId, kind: &str) -> sys::JSValue {
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
        bubbles: true,
        cancelable: true,
        default_prevented: false,
        propagation_stopped: false,
    };
    sys::JS_SetOpaque(e, Box::into_raw(Box::new(s)) as *mut c_void);
    e
}

unsafe fn listeners(ctx: *mut sys::JSContext, node: sys::JSValue) -> sys::JSValue {
    let existing = sys::JS_GetPropertyStr(ctx, node, LISTENERS_PROP.as_ptr() as *const _);
    if existing.tag != sys::JS_TAG_UNDEFINED {
        return existing;
    }
    sys::JS_FreeValue(ctx, existing);
    let created = sys::JS_NewObject(ctx);
    sys::JS_SetPropertyStr(
        ctx,
        node,
        LISTENERS_PROP.as_ptr() as *const _,
        sys::JS_DupValue(ctx, created),
    );
    created
}
unsafe extern "C" fn add(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return sys::js_undefined();
    }
    let Some(kind) = read_string(ctx, *argv) else {
        return type_error(ctx, "event type must be a string");
    };
    let callback = *argv.add(1);
    if !sys::JS_IsFunction(ctx, callback) {
        return type_error(ctx, "event listener must be a function");
    }
    let all = listeners(ctx, node);
    let name = CString::new(kind).unwrap_or_default();
    let old = sys::JS_GetPropertyStr(ctx, all, name.as_ptr());
    let list = if sys::JS_IsArray(old) {
        old
    } else {
        sys::JS_FreeValue(ctx, old);
        let a = sys::JS_NewArray(ctx);
        sys::JS_SetPropertyStr(ctx, all, name.as_ptr(), sys::JS_DupValue(ctx, a));
        a
    };
    let mut len = 0;
    sys::JS_GetLength(ctx, list, &mut len);
    if len < MAX_LISTENERS_PER_TYPE {
        sys::JS_SetPropertyUint32(ctx, list, len as u32, sys::JS_DupValue(ctx, callback));
    }
    sys::JS_FreeValue(ctx, list);
    sys::JS_FreeValue(ctx, all);
    sys::js_undefined()
}
unsafe extern "C" fn remove(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let Some(kind) = read_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let all = listeners(ctx, node);
    let name = CString::new(kind).unwrap_or_default();
    if argc < 2 {
        sys::JS_SetPropertyStr(ctx, all, name.as_ptr(), sys::js_undefined());
    } else {
        let list = sys::JS_GetPropertyStr(ctx, all, name.as_ptr());
        if sys::JS_IsArray(list) {
            let callback = *argv.add(1);
            let mut len = 0;
            sys::JS_GetLength(ctx, list, &mut len);
            for i in 0..len as u32 {
                let registered = sys::JS_GetPropertyUint32(ctx, list, i);
                if sys::JS_IsStrictEqual(ctx, registered, callback) {
                    sys::JS_SetPropertyUint32(ctx, list, i, sys::js_undefined());
                    sys::JS_FreeValue(ctx, registered);
                    break;
                }
                sys::JS_FreeValue(ctx, registered)
            }
        }
        sys::JS_FreeValue(ctx, list)
    }
    sys::JS_FreeValue(ctx, all);
    sys::js_undefined()
}
unsafe fn dispatch_at(ctx: *mut sys::JSContext, node: sys::JSValue, event: sys::JSValue) {
    let p = state(ctx, event);
    let all = listeners(ctx, node);
    let name = CString::new((*p).event_type.as_str()).unwrap_or_default();
    let list = sys::JS_GetPropertyStr(ctx, all, name.as_ptr());
    sys::JS_FreeValue(ctx, all);
    if !sys::JS_IsArray(list) {
        sys::JS_FreeValue(ctx, list);
        return;
    }
    let mut len = 0;
    sys::JS_GetLength(ctx, list, &mut len);
    let mut snapshot = Vec::new();
    for i in 0..len as u32 {
        let f = sys::JS_GetPropertyUint32(ctx, list, i);
        if f.tag != sys::JS_TAG_UNDEFINED {
            snapshot.push(f)
        } else {
            sys::JS_FreeValue(ctx, f)
        }
    }
    sys::JS_FreeValue(ctx, list);
    for f in snapshot {
        let mut args = [sys::JS_DupValue(ctx, event)];
        let r = sys::JS_Call(ctx, f, node, 1, args.as_mut_ptr());
        sys::JS_FreeValue(ctx, r);
        sys::JS_FreeValue(ctx, args[0]);
        sys::JS_FreeValue(ctx, f)
    }
}
pub(crate) unsafe fn dispatch(ctx: *mut sys::JSContext, node: sys::JSValue, kind: &str) -> bool {
    let Some(target_id) = crate::dom_bindings::node_id(ctx, node) else {
        return true;
    };
    let event = make_event(ctx, target_id, kind);
    if sys::js_is_exception(&event) {
        return false;
    }
    let mut current = Some(target_id);
    while let Some(id) = current {
        let p = state(ctx, event);
        if (*p).propagation_stopped {
            break;
        }
        (*p).current_target = Some(id);
        let current_node = crate::dom_bindings::node_object(
            ctx,
            crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), "Node"),
            id,
        );
        dispatch_at(ctx, current_node, event);
        sys::JS_FreeValue(ctx, current_node);
        current = crate::dom_bindings::parent_node_id(ctx, id);
    }
    let canceled = (*state(ctx, event)).default_prevented;
    sys::JS_FreeValue(ctx, event);
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
    let Some(kind) = read_string(ctx, *argv) else {
        return type_error(ctx, "event type must be a string");
    };
    sys::js_bool(dispatch(ctx, node, &kind))
}
pub(crate) unsafe fn define_event_target(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    for (name, func, arity) in [
        ("addEventListener", add as sys::JSCFunction, 2),
        ("removeEventListener", remove as sys::JSCFunction, 1),
        ("dispatchEvent", dispatch_event as sys::JSCFunction, 1),
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
