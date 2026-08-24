//! Bounded DOM-style event dispatch for `Node` objects.
//!
//! Event state is native structured data. User code is invoked only as a
//! QuickJS function; no supplied string is parsed or evaluated by this layer.
use quickjs_sys as sys;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

const LISTENERS_PROP: &[u8] = b"__listeners\0";
const MAX_LISTENERS_PER_TYPE: i64 = 64;
const MAX_BUBBLE_DEPTH: usize = 128;
const MAX_NESTED_DISPATCH: usize = 32;
const EVENT_CLASS_KIND: &str = "Event";

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

struct EventState {
    event_type: String,
    target: Option<dom::NodeId>,
    current_target: Option<dom::NodeId>,
    bubbles: bool,
    cancelable: bool,
    default_prevented: bool,
    propagation_stopped: bool,
    /// Set only while a `passive: true` listener's callback is on the stack
    /// (see `call_record`) — real passive-listener semantics: calling
    /// `preventDefault()` from inside one is silently ignored rather than
    /// actually canceling the event, regardless of `cancelable`.
    in_passive_listener: bool,
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
unsafe extern "C" fn prevent_default(
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

unsafe fn make_event(
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

/// Real `EventListener` shape per spec: either a plain callable, or an
/// object exposing a callable `handleEvent` method (invoked with the
/// listener object itself as `this`, per spec — not the node).
unsafe fn is_valid_listener(ctx: *mut sys::JSContext, value: sys::JSValue) -> bool {
    if sys::JS_IsFunction(ctx, value) {
        return true;
    }
    if value.tag != sys::JS_TAG_OBJECT {
        return false;
    }
    let name = CString::new("handleEvent").unwrap();
    let handler = sys::JS_GetPropertyStr(ctx, value, name.as_ptr());
    let ok = sys::JS_IsFunction(ctx, handler);
    sys::JS_FreeValue(ctx, handler);
    ok
}

/// Real `addEventListener`/`removeEventListener` third-argument shape:
/// either a bare `useCapture` boolean, or an options object with
/// `capture`/`once`/`passive`. `signal` (`AbortSignal`) isn't read — this
/// crate has no `AbortController`/`AbortSignal` implementation yet, a
/// documented scope cut rather than a silent no-op (nothing here claims to
/// honor it).
struct ListenerOptions {
    capture: bool,
    once: bool,
    passive: bool,
}
unsafe fn read_listener_options(
    ctx: *mut sys::JSContext,
    argc: c_int,
    argv: *mut sys::JSValue,
    index: usize,
) -> ListenerOptions {
    if argc <= index as c_int {
        return ListenerOptions {
            capture: false,
            once: false,
            passive: false,
        };
    }
    let value = *argv.add(index);
    if value.tag != sys::JS_TAG_OBJECT {
        return ListenerOptions {
            capture: sys::JS_ToBool(ctx, value) != 0,
            once: false,
            passive: false,
        };
    }
    let read = |name: &str| {
        let cname = CString::new(name).unwrap();
        let prop = sys::JS_GetPropertyStr(ctx, value, cname.as_ptr());
        let b = sys::JS_ToBool(ctx, prop) != 0;
        sys::JS_FreeValue(ctx, prop);
        b
    };
    ListenerOptions {
        capture: read("capture"),
        once: read("once"),
        passive: read("passive"),
    }
}

const RECORD_CALLBACK: &[u8] = b"callback\0";
const RECORD_CAPTURE: &[u8] = b"capture\0";
const RECORD_ONCE: &[u8] = b"once\0";
const RECORD_PASSIVE: &[u8] = b"passive\0";

/// Wraps one registered listener with its options into a plain JS object —
/// this crate stores listener state as native QuickJS values (no extra
/// native class), so a record is just `{callback, capture, once, passive}`
/// pushed into the same per-type array `dispatch_at`/`remove` already used
/// for bare callbacks.
unsafe fn make_record(
    ctx: *mut sys::JSContext,
    callback: sys::JSValue,
    options: &ListenerOptions,
) -> sys::JSValue {
    let record = sys::JS_NewObject(ctx);
    sys::JS_SetPropertyStr(ctx, record, RECORD_CALLBACK.as_ptr() as *const _, sys::JS_DupValue(ctx, callback));
    sys::JS_SetPropertyStr(ctx, record, RECORD_CAPTURE.as_ptr() as *const _, sys::js_bool(options.capture));
    sys::JS_SetPropertyStr(ctx, record, RECORD_ONCE.as_ptr() as *const _, sys::js_bool(options.once));
    sys::JS_SetPropertyStr(ctx, record, RECORD_PASSIVE.as_ptr() as *const _, sys::js_bool(options.passive));
    record
}
unsafe fn record_callback(ctx: *mut sys::JSContext, record: sys::JSValue) -> sys::JSValue {
    sys::JS_GetPropertyStr(ctx, record, RECORD_CALLBACK.as_ptr() as *const _)
}
unsafe fn record_bool(ctx: *mut sys::JSContext, record: sys::JSValue, name: &[u8]) -> bool {
    let value = sys::JS_GetPropertyStr(ctx, record, name.as_ptr() as *const _);
    let result = sys::JS_ToBool(ctx, value) != 0;
    sys::JS_FreeValue(ctx, value);
    result
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
    if !is_valid_listener(ctx, callback) {
        return type_error(ctx, "event listener must be a function or an object with handleEvent");
    }
    let options = read_listener_options(ctx, argc, argv, 2);
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
    if len >= MAX_LISTENERS_PER_TYPE {
        sys::JS_FreeValue(ctx, list);
        sys::JS_FreeValue(ctx, all);
        return type_error(ctx, "event listener limit exceeded");
    }
    let record = make_record(ctx, callback, &options);
    sys::JS_SetPropertyUint32(ctx, list, len as u32, record);
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
            let capture = read_listener_options(ctx, argc, argv, 2).capture;
            let mut len = 0;
            sys::JS_GetLength(ctx, list, &mut len);
            for i in 0..len as u32 {
                let record = sys::JS_GetPropertyUint32(ctx, list, i);
                if record.tag == sys::JS_TAG_UNDEFINED {
                    sys::JS_FreeValue(ctx, record);
                    continue;
                }
                let registered = record_callback(ctx, record);
                let matches = sys::JS_IsStrictEqual(ctx, registered, callback)
                    && record_bool(ctx, record, RECORD_CAPTURE) == capture;
                sys::JS_FreeValue(ctx, registered);
                sys::JS_FreeValue(ctx, record);
                if matches {
                    sys::JS_SetPropertyUint32(ctx, list, i, sys::js_undefined());
                    break;
                }
            }
        }
        sys::JS_FreeValue(ctx, list)
    }
    sys::JS_FreeValue(ctx, all);
    sys::js_undefined()
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

/// Removes the first record in `node`'s `event_type` list matching
/// `callback`+`capture` — used to drop a `once` listener right after it
/// fires. Sets the slot to `undefined` in place (same sparse-array
/// convention `remove()` already uses) rather than splicing, so this never
/// shifts indices a concurrent iteration snapshot elsewhere still relies on.
unsafe fn remove_matching_record(
    ctx: *mut sys::JSContext,
    node: sys::JSValue,
    event_type: &str,
    callback: sys::JSValue,
    capture: bool,
) {
    let all = listeners(ctx, node);
    let name = CString::new(event_type).unwrap_or_default();
    let list = sys::JS_GetPropertyStr(ctx, all, name.as_ptr());
    sys::JS_FreeValue(ctx, all);
    if sys::JS_IsArray(list) {
        let mut len = 0;
        sys::JS_GetLength(ctx, list, &mut len);
        for i in 0..len as u32 {
            let record = sys::JS_GetPropertyUint32(ctx, list, i);
            if record.tag == sys::JS_TAG_UNDEFINED {
                sys::JS_FreeValue(ctx, record);
                continue;
            }
            let registered = record_callback(ctx, record);
            let matches = sys::JS_IsStrictEqual(ctx, registered, callback)
                && record_bool(ctx, record, RECORD_CAPTURE) == capture;
            sys::JS_FreeValue(ctx, registered);
            sys::JS_FreeValue(ctx, record);
            if matches {
                sys::JS_SetPropertyUint32(ctx, list, i, sys::js_undefined());
                break;
            }
        }
    }
    sys::JS_FreeValue(ctx, list);
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
        let capture = record_bool(ctx, record, RECORD_CAPTURE);
        if !record_matches_phase(phase, capture) {
            sys::JS_FreeValue(ctx, record);
            continue;
        }
        snapshot.push(Snapshot {
            callback: record_callback(ctx, record),
            capture,
            once: record_bool(ctx, record, RECORD_ONCE),
            passive: record_bool(ctx, record, RECORD_PASSIVE),
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
    let node_class = crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), "Node");
    let run_at = |ctx: *mut sys::JSContext, id: dom::NodeId, phase: Phase, first_exception: &mut Option<sys::JSValue>| {
        let p = state(ctx, event);
        (*p).current_target = Some(id);
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
