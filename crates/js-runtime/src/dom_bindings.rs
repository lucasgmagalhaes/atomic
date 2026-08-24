//! DOM↔JS bindings: a `Node` JS class wrapping a boxed `dom::NodeId` as its
//! opaque data, plus `document.getElementById(id)` and the `textContent`
//! accessor on `Node.prototype`. Backed by a `dom::Dom` stashed in the
//! context's opaque slot (see `Context::with_dom`).
//!
//! Real per-`dom::NodeId` object identity now (`get_or_create_node_object`,
//! `NODE_OBJECTS` below) — a genuine bug this fixes, not a new feature:
//! `document.getElementById(id)` used to build a brand-new `Node` JS
//! object on every single call, so `el.addEventListener(...)` followed by
//! a *separate* `document.getElementById(id)` call later (any real page
//! calling it twice — extremely common; a listener attached at page-load
//! time and dispatched from a later, separate `eval()`, e.g. a real
//! coordinate click routed through `profile-worker`) landed the listener
//! on a wrapper object that was never seen again, since
//! `events.rs`'s `__listeners` storage lives as an own property on the
//! wrapper instance, not keyed by `NodeId` itself. Caching one JS object
//! per real `NodeId` (thread-local, keyed by `JSContext` pointer then
//! `NodeId` — same convention as `timers`/`fetch_async`'s own registries)
//! gives every lookup of the same DOM node the same JS object identity, so
//! its properties (listeners included) actually persist the way a real
//! `Node`'s object identity does.
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use quickjs_sys as sys;

thread_local! {
    static NODE_OBJECTS: RefCell<HashMap<usize, HashMap<dom::NodeId, sys::JSValue>>> = RefCell::new(HashMap::new());
}

/// Returns a real, identity-stable `Node` JS object for `id` — the same
/// `JSValue` every prior call for this `(ctx, id)` pair returned, not a
/// fresh one. Each call hands back a new *owned reference* to that same
/// object (`JS_DupValue`), matching every other binding in this crate's
/// own "caller owns what it gets back" convention — the cache itself
/// holds one reference for as long as the `Context` lives, freed by
/// [`cleanup`].
unsafe fn get_or_create_node_object(ctx: *mut sys::JSContext, class_id: sys::JSClassID, id: dom::NodeId) -> sys::JSValue {
    let ctx_key = ctx as usize;
    let cached = NODE_OBJECTS.with(|reg| reg.borrow().get(&ctx_key).and_then(|nodes| nodes.get(&id).copied()));
    if let Some(obj) = cached {
        return sys::JS_DupValue(ctx, obj);
    }

    let obj = make_node_object(ctx, class_id, id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    NODE_OBJECTS.with(|reg| {
        reg.borrow_mut().entry(ctx_key).or_default().insert(id, sys::JS_DupValue(ctx, obj));
    });
    obj
}

/// Frees every cached `Node` object for `ctx` — must run before
/// `JS_FreeContext` (same ordering requirement `timers::cleanup`/
/// `fetch_async::cleanup` already document), since a `JSValue` can't be
/// freed against an already-freed context.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    if let Some(nodes) = NODE_OBJECTS.with(|reg| reg.borrow_mut().remove(&(ctx as usize))) {
        for (_, obj) in nodes {
            sys::JS_FreeValue(ctx, obj);
        }
    }
}

/// See `crate::class_registry` - one registry entry per `JSRuntime`, not
/// a single value shared across every `Runtime` in the process.
const NODE_CLASS_KIND: &str = "Node";

unsafe fn read_js_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
    let mut len: usize = 0;
    let ptr = sys::JS_ToCStringLen2(ctx, &mut len, val, false);
    if ptr.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
    let s = String::from_utf8_lossy(bytes).into_owned();
    sys::JS_FreeCString(ctx, ptr);
    Some(s)
}

unsafe fn new_js_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const std::os::raw::c_char, s.len())
}

unsafe fn node_opaque(rt: *mut sys::JSRuntime, this_val: sys::JSValue) -> *mut dom::NodeId {
    let class_id = crate::class_registry::class_id_for(rt, NODE_CLASS_KIND);
    sys::JS_GetOpaque(this_val, class_id) as *mut dom::NodeId
}

unsafe fn dom_opaque(ctx: *mut sys::JSContext) -> *mut dom::Dom {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return std::ptr::null_mut();
    }
    // A raw pointer to a field within the boxed `HostState` - sound as
    // long as the box isn't moved, which `Context` guarantees the same
    // way it already did when this pointed straight at a boxed `dom::Dom`
    // (see `Context`'s own doc comment on why moving the `Box` doesn't
    // move the heap allocation it points to).
    std::ptr::addr_of_mut!((*state).dom)
}

unsafe extern "C" fn node_finalizer(rt: *mut sys::JSRuntime, val: sys::JSValue) {
    let ptr = node_opaque(rt, val);
    if !ptr.is_null() {
        drop(Box::from_raw(ptr));
    }
}

type Getter = unsafe extern "C" fn(ctx: *mut sys::JSContext, this_val: sys::JSValue) -> sys::JSValue;
type Setter = unsafe extern "C" fn(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue;

unsafe extern "C" fn node_text_content_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if node_ptr.is_null() || dom_ptr.is_null() {
        return sys::js_undefined();
    }
    new_js_string(ctx, &(*dom_ptr).text_content(*node_ptr))
}

unsafe extern "C" fn node_text_content_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    let Some(text) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        (*dom_ptr).set_text_content(*node_ptr, &text);
    }
    sys::js_undefined()
}

/// Defines the `textContent` accessor on `proto`. Takes ownership of `proto`
/// only in the sense of mutating it in place — callers still own the value.
unsafe fn define_text_content(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("textContent").unwrap();

    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(node_text_content_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let setter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(node_text_content_set),
        name.as_ptr(),
        1,
        sys::JS_CFUNC_SETTER,
        0,
    );

    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        setter,
        sys::JS_PROP_HAS_GET | sys::JS_PROP_HAS_SET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}

unsafe extern "C" fn node_value_get(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if node_ptr.is_null() || dom_ptr.is_null() {
        return sys::js_undefined();
    }
    new_js_string(ctx, &(*dom_ptr).value(*node_ptr))
}

unsafe extern "C" fn node_value_set(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    val: sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    let Some(text) = read_js_string(ctx, val) else {
        return sys::js_undefined();
    };
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        (*dom_ptr).set_value(*node_ptr, &text);
        // Real `"input"` semantics: fires on every value mutation,
        // regardless of whether it came from a user keystroke
        // (`profile-worker`'s `type_key`) or a script setting `.value =`
        // directly — matches the real DOM, which doesn't distinguish the
        // two for this event.
        crate::events::dispatch(ctx, this_val, "input");
    }
    sys::js_undefined()
}

/// Defines the `value` accessor on `proto` — real, independent of
/// `textContent` (see `dom::Dom::value`/`set_value`'s own docs for the
/// deviation from a typed `HTMLInputElement`/`HTMLTextAreaElement`
/// hierarchy this generic `Node` class makes).
unsafe fn define_value(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let name = CString::new("value").unwrap();

    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(node_value_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let setter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Setter, sys::JSCFunction>(node_value_set),
        name.as_ptr(),
        1,
        sys::JS_CFUNC_SETTER,
        0,
    );

    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(
        ctx,
        proto,
        atom,
        getter,
        setter,
        sys::JS_PROP_HAS_GET | sys::JS_PROP_HAS_SET | sys::JS_PROP_CONFIGURABLE | sys::JS_PROP_ENUMERABLE,
    );
    sys::JS_FreeAtom(ctx, atom);
}

unsafe extern "C" fn node_focus(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        (*dom_ptr).focus(*node_ptr);
        crate::events::dispatch(ctx, this_val, "focus");
    }
    sys::js_undefined()
}

unsafe extern "C" fn node_blur(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
    _argc: c_int,
    _argv: *mut sys::JSValue,
) -> sys::JSValue {
    let node_ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    let dom_ptr = dom_opaque(ctx);
    if !node_ptr.is_null() && !dom_ptr.is_null() {
        // `Some(changed)` only when `id` really was the focused node —
        // matches the real DOM not firing `"blur"` for an element that
        // wasn't focused to begin with.
        if let Some(changed) = (*dom_ptr).blur(*node_ptr) {
            crate::events::dispatch(ctx, this_val, "blur");
            if changed {
                crate::events::dispatch(ctx, this_val, "change");
            }
        }
    }
    sys::js_undefined()
}

/// Defines real `focus()`/`blur()` methods on `proto`, backed by
/// `dom::Dom`'s own focus state (`document.activeElement`, see
/// `document_active_element_get` below).
unsafe fn define_focus_methods(ctx: *mut sys::JSContext, proto: sys::JSValue) {
    let focus_name = CString::new("focus").unwrap();
    let focus_fn = sys::JS_NewCFunction2(ctx, node_focus, focus_name.as_ptr(), 0, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, proto, focus_name.as_ptr(), focus_fn);

    let blur_name = CString::new("blur").unwrap();
    let blur_fn = sys::JS_NewCFunction2(ctx, node_blur, blur_name.as_ptr(), 0, sys::JS_CFUNC_GENERIC, 0);
    sys::JS_SetPropertyStr(ctx, proto, blur_name.as_ptr(), blur_fn);
}

/// Registers the `Node` class on `ctx`'s runtime (if not already done for
/// this runtime) and builds this context's `Node.prototype`.
unsafe fn ensure_node_class(ctx: *mut sys::JSContext) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    let class_name = CString::new("Node").unwrap();
    let def = sys::JSClassDef {
        class_name: class_name.as_ptr(),
        finalizer: Some(node_finalizer),
        gc_mark: std::ptr::null_mut(),
        call: std::ptr::null_mut(),
        exotic: std::ptr::null_mut(),
    };
    let class_id = crate::class_registry::ensure_class(rt, NODE_CLASS_KIND, &def);

    let proto = sys::JS_NewObject(ctx);
    define_text_content(ctx, proto);
    define_value(ctx, proto);
    define_focus_methods(ctx, proto);
    crate::events::define_event_target(ctx, proto);
    sys::JS_SetClassProto(ctx, class_id, proto);

    class_id
}

unsafe fn make_node_object(ctx: *mut sys::JSContext, class_id: sys::JSClassID, id: dom::NodeId) -> sys::JSValue {
    let obj = sys::JS_NewObjectClass(ctx, class_id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    sys::JS_SetOpaque(obj, Box::into_raw(Box::new(id)) as *mut c_void);
    obj
}

unsafe extern "C" fn document_get_element_by_id(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_null();
    }
    let Some(id) = read_js_string(ctx, *argv) else {
        return sys::js_null();
    };
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::js_null();
    }
    match (*dom_ptr).find_by_id(&id) {
        Some(node_id) => get_or_create_node_object(ctx, crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND), node_id),
        None => sys::js_null(),
    }
}

unsafe extern "C" fn document_active_element_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
) -> sys::JSValue {
    let dom_ptr = dom_opaque(ctx);
    if dom_ptr.is_null() {
        return sys::js_null();
    }
    match (*dom_ptr).active_element() {
        Some(node_id) => get_or_create_node_object(ctx, crate::class_registry::class_id_for(sys::JS_GetRuntime(ctx), NODE_CLASS_KIND), node_id),
        None => sys::js_null(),
    }
}

/// Defines the real, read-only `document.activeElement` getter — backed by
/// `dom::Dom`'s own focus state, so it reflects whatever the most recent
/// `.focus()`/`.blur()` call (JS-driven or `profile-worker`'s coordinate
/// click routing) left focused.
unsafe fn define_active_element(ctx: *mut sys::JSContext, document: sys::JSValue) {
    let name = CString::new("activeElement").unwrap();
    let getter = sys::JS_NewCFunction2(
        ctx,
        std::mem::transmute::<Getter, sys::JSCFunction>(document_active_element_get),
        name.as_ptr(),
        0,
        sys::JS_CFUNC_GETTER,
        0,
    );
    let atom = sys::JS_NewAtom(ctx, name.as_ptr());
    sys::JS_DefinePropertyGetSet(ctx, document, atom, getter, sys::js_undefined(), sys::JS_PROP_HAS_GET | sys::JS_PROP_CONFIGURABLE);
    sys::JS_FreeAtom(ctx, atom);
}

/// Registers the `Node` class and a global `document` object exposing
/// `getElementById(id)`/`activeElement`. Callers must have already pointed
/// the context's opaque slot at a live `dom::Dom` via `JS_SetContextOpaque`
/// — bindings read it back on every call and no-op (return null/undefined)
/// if unset.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    ensure_node_class(ctx);

    let document = crate::document::get_or_create(ctx);

    let name = CString::new("getElementById").unwrap();
    let get_by_id = sys::JS_NewCFunction2(
        ctx,
        document_get_element_by_id,
        name.as_ptr(),
        1,
        sys::JS_CFUNC_GENERIC,
        0,
    );
    sys::JS_SetPropertyStr(ctx, document, name.as_ptr(), get_by_id);
    define_active_element(ctx, document);

    sys::JS_FreeValue(ctx, document);
}
