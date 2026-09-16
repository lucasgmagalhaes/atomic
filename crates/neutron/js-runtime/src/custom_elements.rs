//! `customElements` (`ROADMAP.md` item 39: Custom Elements).
//!
//! Real `customElements.define(name, ctor)`/`.get(name)` with the spec's
//! hyphenated-name validity check (plus the small reserved-name
//! exclusion list), and real lifecycle callback invocation:
//! `connectedCallback()`/`disconnectedCallback()` are called (with the
//! element's own node object as `this`) whenever a registered tag's
//! element is actually connected to/disconnected from the document —
//! hooked into every tree-mutating method that can change connectedness
//! (`appendChild`/`insertBefore`/`removeChild`/`remove`/`replaceChild`,
//! see `dom_bindings::tree_edit`). `define()` itself also upgrades every
//! already-connected matching element already in the document (real
//! spec behavior: defining a name after matching elements exist in the
//! DOM still triggers their `connectedCallback` — the common real-world
//! order, since this crate's scripts run after parsing).
//!
//! Scope cut, deliberate: this engine has one generic `Node` JS class
//! hierarchy (`HTMLElement` etc., see `dom_bindings::element_classes`),
//! not a per-tag class hierarchy — `document.createElement('my-el')`
//! still returns a plain `HTMLElement` instance, not one whose prototype
//! chain includes the registered constructor's own prototype (real
//! "custom element upgrade" replaces the object's class entirely, so
//! `el instanceof MyEl` stays `false` here). The lifecycle callback
//! itself, though, is read directly from the *registered constructor's*
//! `.prototype` (not the element's own object) and invoked with the
//! connected/disconnected element as `this` — the ordinary
//! `class MyEl extends HTMLElement { connectedCallback() { ... } }`
//! pattern works with no extra step, it just isn't backed by a real
//! prototype-chain substitution. `adoptedCallback`/
//! `attributeChangedCallback`/`observedAttributes` aren't modeled.
//! Reactions fire synchronously at the mutation point, not queued as a
//! real microtask — this crate has no dedicated custom-element reaction
//! queue, matching `events::dispatch_simple`'s own "call synchronously,
//! no microtask queue for this yet" precedent.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_int;

use quickjs_sys as sys;

thread_local! {
    // Keyed by JSContext pointer then registered name, same convention
    // every other per-context registry in this crate uses.
    static REGISTRIES: RefCell<HashMap<usize, HashMap<String, sys::JSValue>>> =
        RefCell::new(HashMap::new());
}

const RESERVED_NAMES: &[&str] = &[
    "annotation-xml",
    "color-profile",
    "font-face",
    "font-face-src",
    "font-face-uri",
    "font-face-format",
    "font-face-name",
    "missing-glyph",
];

unsafe fn new_string(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_NewStringLen(ctx, s.as_ptr() as *const _, s.len())
}
unsafe fn throw_type_error(ctx: *mut sys::JSContext, s: &str) -> sys::JSValue {
    sys::JS_Throw(ctx, new_string(ctx, s))
}
unsafe fn read_string(ctx: *mut sys::JSContext, val: sys::JSValue) -> Option<String> {
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

/// Real spec-shaped validity check for an autonomous custom element name:
/// lowercase ASCII letters/digits/`-`/`_`/`.` only, starting with a
/// lowercase letter, containing at least one hyphen (the spec's own
/// distinguishing rule from a built-in element name), and not one of the
/// small reserved SVG/MathML-shadowing names. Not the full spec table
/// (Unicode PCENChar ranges) — real but narrower, same convention every
/// other validation in this crate already documents.
fn is_valid_custom_element_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    if !name.contains('-') || RESERVED_NAMES.contains(&name) {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

fn is_registered(ctx: *mut sys::JSContext, name: &str) -> bool {
    REGISTRIES.with(|reg| {
        reg.borrow()
            .get(&(ctx as usize))
            .map(|m| m.contains_key(name))
            .unwrap_or(false)
    })
}

unsafe extern "C" fn custom_elements_define(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 2 {
        return throw_type_error(ctx, "customElements.define: 2 arguments required");
    }
    let Some(name) = read_string(ctx, *argv) else {
        return throw_type_error(ctx, "customElements.define: name must be a string");
    };
    if !is_valid_custom_element_name(&name) {
        return throw_type_error(
            ctx,
            &format!("customElements.define: '{name}' is not a valid custom element name"),
        );
    }
    let ctor = *argv.add(1);
    if !sys::JS_IsFunction(ctx, ctor) {
        return throw_type_error(ctx, "customElements.define: constructor must be a function");
    }
    if is_registered(ctx, &name) {
        return throw_type_error(
            ctx,
            &format!("customElements.define: '{name}' has already been defined"),
        );
    }
    REGISTRIES.with(|reg| {
        reg.borrow_mut()
            .entry(ctx as usize)
            .or_default()
            .insert(name.clone(), sys::JS_DupValue(ctx, ctor));
    });
    upgrade_existing(ctx, &name);
    sys::js_undefined()
}

unsafe extern "C" fn custom_elements_get(
    ctx: *mut sys::JSContext,
    _this_val: sys::JSValue,
    argc: c_int,
    argv: *mut sys::JSValue,
) -> sys::JSValue {
    if argc < 1 {
        return sys::js_undefined();
    }
    let Some(name) = read_string(ctx, *argv) else {
        return sys::js_undefined();
    };
    let found = REGISTRIES.with(|reg| {
        reg.borrow()
            .get(&(ctx as usize))
            .and_then(|m| m.get(&name).copied())
    });
    match found {
        Some(v) => sys::JS_DupValue(ctx, v),
        None => sys::js_undefined(),
    }
}

/// Looks up `name`'s registered constructor without transferring
/// ownership — callers must never `JS_FreeValue` the result, only use it
/// to read further properties off it (matching the registry's own
/// internal reference, which `cleanup`/re-`define` free).
fn get_ctor(ctx: *mut sys::JSContext, name: &str) -> Option<sys::JSValue> {
    REGISTRIES.with(|reg| {
        reg.borrow()
            .get(&(ctx as usize))
            .and_then(|m| m.get(name).copied())
    })
}

/// Calls `id`'s registered custom element constructor's own
/// `.prototype.connectedCallback`/`.disconnectedCallback` (if present),
/// with `id`'s own node object as `this` — see the module doc for why
/// the callback is read from the constructor's prototype rather than the
/// node's own object.
unsafe fn fire_lifecycle_callback(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
    callback_name: &[u8],
) {
    if dom.is_null() {
        return;
    }
    let tag = match (*dom).get(id).map(|n| &n.data) {
        Some(dom::NodeData::Element { tag, .. }) => tag.as_str().to_string(),
        _ => return,
    };
    let Some(ctor) = get_ctor(ctx, &tag) else {
        return;
    };
    let proto = sys::JS_GetPropertyStr(ctx, ctor, b"prototype\0".as_ptr() as *const _);
    let method = sys::JS_GetPropertyStr(ctx, proto, callback_name.as_ptr() as *const _);
    sys::JS_FreeValue(ctx, proto);
    if !sys::JS_IsFunction(ctx, method) {
        sys::JS_FreeValue(ctx, method);
        return;
    }
    let class_id = crate::dom_bindings::node_class_id_for(ctx, dom, id);
    let obj = crate::dom_bindings::node_object(ctx, class_id, id);
    if sys::js_is_exception(&obj) {
        sys::JS_FreeValue(ctx, method);
        sys::JS_FreeValue(ctx, obj);
        return;
    }
    let result = sys::JS_Call(ctx, method, obj, 0, std::ptr::null_mut());
    sys::JS_FreeValue(ctx, result);
    sys::JS_FreeValue(ctx, method);
    sys::JS_FreeValue(ctx, obj);
}

/// Called after a mutation that may have connected `id` to the document
/// — fires `connectedCallback` if it's now actually connected and its
/// tag is registered. A no-op for anything else (unregistered tag, or a
/// mutation that didn't actually result in `id` being connected — e.g.
/// appending into an already-detached subtree).
pub(crate) unsafe fn maybe_connected(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
) {
    if dom.is_null() || !(*dom).is_connected(id) {
        return;
    }
    fire_lifecycle_callback(ctx, dom, id, b"connectedCallback\0");
}

/// Called after a mutation that disconnected `id` from the document —
/// `was_connected` must be captured by the caller *before* the mutation
/// ran (this function can't tell "was connected, now isn't" from "was
/// never connected" on its own once the mutation has already happened).
pub(crate) unsafe fn maybe_disconnected(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
    was_connected: bool,
) {
    if !was_connected {
        return;
    }
    fire_lifecycle_callback(ctx, dom, id, b"disconnectedCallback\0");
}

/// Real "upgrade already-connected elements matching a name just
/// defined" — walks the whole document from its root, firing
/// `connectedCallback` on every matching, currently-connected element.
unsafe fn upgrade_existing(ctx: *mut sys::JSContext, name: &str) {
    let state = crate::host_state::get(ctx);
    if state.is_null() {
        return;
    }
    let dom_ptr = &mut (*state).dom as *mut dom::Dom;
    let root = (*dom_ptr).root();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let Some(node) = (*dom_ptr).get(id) else {
            continue;
        };
        if let dom::NodeData::Element { tag, .. } = &node.data {
            if tag.as_str() == name {
                fire_lifecycle_callback(ctx, dom_ptr, id, b"connectedCallback\0");
            }
        }
        stack.extend(node.children.iter().copied());
    }
}

/// Registers the global `customElements` object (`define`/`get`).
/// Unconditional, same as every other global this crate registers in
/// [`crate::Context::new`] — a plain `Context::new` without a `dom` has
/// no host state behind [`upgrade_existing`]/[`fire_lifecycle_callback`],
/// which degrade to no-ops, same pattern every other `HostState`-backed
/// feature in this crate already follows.
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    let obj = sys::JS_NewObject(ctx);
    crate::js_helpers::define_method(ctx, obj, "define", custom_elements_define, 2);
    crate::js_helpers::define_method(ctx, obj, "get", custom_elements_get, 1);
    let global = sys::JS_GetGlobalObject(ctx);
    let name = CString::new("customElements").unwrap();
    sys::JS_SetPropertyStr(ctx, global, name.as_ptr(), obj);
    sys::JS_FreeValue(ctx, global);
}

/// Frees every registered constructor for `ctx` — must run before
/// `JS_FreeContext`, same ordering requirement every other module's
/// `cleanup(ctx)` already documents.
pub(crate) unsafe fn cleanup(ctx: *mut sys::JSContext) {
    crate::js_helpers::cleanup_object_cache(&REGISTRIES, ctx);
}
