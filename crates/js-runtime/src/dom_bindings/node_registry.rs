//! Base layer every other `dom_bindings` submodule depends on: the real
//! per-`dom::NodeId` JS object identity cache (`NODE_OBJECTS`,
//! `get_or_create`-style [`node_object`]), the opaque-slot readers
//! (`node_id`/`node_opaque`/`dom_opaque`), and the class-kind lookup used to
//! pick the right JS class for a given DOM node ([`node_class_id_for`]).
//!
//! Real per-`dom::NodeId` object identity (`node_object`, `NODE_OBJECTS`
//! below) — a genuine bug this fixes, not a new feature:
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

use quickjs_sys as sys;

use super::{attributes, class_list, dataset, element_classes};

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
pub(crate) unsafe fn node_object(
    ctx: *mut sys::JSContext,
    class_id: sys::JSClassID,
    id: dom::NodeId,
) -> sys::JSValue {
    let ctx_key = ctx as usize;
    let cached = NODE_OBJECTS.with(|reg| {
        reg.borrow()
            .get(&ctx_key)
            .and_then(|nodes| nodes.get(&id).copied())
    });
    if let Some(obj) = cached {
        return sys::JS_DupValue(ctx, obj);
    }

    let obj = element_classes::make_node_object(ctx, class_id, id);
    if sys::js_is_exception(&obj) {
        return obj;
    }
    NODE_OBJECTS.with(|reg| {
        reg.borrow_mut()
            .entry(ctx_key)
            .or_default()
            .insert(id, sys::JS_DupValue(ctx, obj));
    });
    obj
}

/// Returns the DOM parent of a node, if the context still has live DOM
/// state. This deliberately exposes IDs rather than DOM references so event
/// dispatch cannot retain or mutate the DOM while walking its propagation path.
pub(crate) unsafe fn parent_node_id(
    ctx: *mut sys::JSContext,
    id: dom::NodeId,
) -> Option<dom::NodeId> {
    let dom = dom_opaque(ctx);
    (!dom.is_null())
        .then(|| (*dom).get(id).and_then(|node| node.parent))
        .flatten()
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
    class_list::cleanup(ctx);
    dataset::cleanup(ctx);
    attributes::cleanup(ctx);
    crate::css_style::cleanup(ctx);
}

/// See `crate::class_registry` - one registry entry per `JSRuntime`, not
/// a single value shared across every `Runtime` in the process.
pub(super) const NODE_CLASS_KIND: &str = "Node";
pub(super) const ELEMENT_CLASS_KIND: &str = "Element";
pub(super) const HTML_ELEMENT_CLASS_KIND: &str = "HTMLElement";
pub(super) const HTML_INPUT_CLASS_KIND: &str = "HTMLInputElement";
pub(super) const HTML_BUTTON_CLASS_KIND: &str = "HTMLButtonElement";
pub(super) const HTML_ANCHOR_CLASS_KIND: &str = "HTMLAnchorElement";
pub(super) const HTML_IMAGE_CLASS_KIND: &str = "HTMLImageElement";
pub(super) const HTML_CANVAS_CLASS_KIND: &str = "HTMLCanvasElement";
pub(super) const HTML_FORM_CLASS_KIND: &str = "HTMLFormElement";
pub(super) const HTML_SELECT_CLASS_KIND: &str = "HTMLSelectElement";

const ALL_NODE_CLASS_KINDS: &[&str] = &[
    NODE_CLASS_KIND,
    ELEMENT_CLASS_KIND,
    HTML_ELEMENT_CLASS_KIND,
    HTML_INPUT_CLASS_KIND,
    HTML_BUTTON_CLASS_KIND,
    HTML_ANCHOR_CLASS_KIND,
    HTML_IMAGE_CLASS_KIND,
    HTML_CANVAS_CLASS_KIND,
    HTML_FORM_CLASS_KIND,
    HTML_SELECT_CLASS_KIND,
];

/// Returns the class ID for a node based on its `NodeData` variant and tag.
/// Elements get their specific subclass (HTMLInputElement, etc.), text/comment/
/// document-fragment nodes get the base `Node` class.
pub(crate) unsafe fn node_class_id_for(
    ctx: *mut sys::JSContext,
    dom: *mut dom::Dom,
    id: dom::NodeId,
) -> sys::JSClassID {
    let rt = sys::JS_GetRuntime(ctx);
    if dom.is_null() {
        return crate::class_registry::class_id_for(rt, NODE_CLASS_KIND);
    }
    match (*dom).get(id).map(|n| &n.data) {
        Some(dom::NodeData::Element { tag, .. }) => {
            let kind = match tag.as_str() {
                "input" => HTML_INPUT_CLASS_KIND,
                "button" => HTML_BUTTON_CLASS_KIND,
                "a" => HTML_ANCHOR_CLASS_KIND,
                "img" => HTML_IMAGE_CLASS_KIND,
                "canvas" => HTML_CANVAS_CLASS_KIND,
                "form" => HTML_FORM_CLASS_KIND,
                "select" => HTML_SELECT_CLASS_KIND,
                _ => HTML_ELEMENT_CLASS_KIND,
            };
            let id = crate::class_registry::class_id_for(rt, kind);
            if id != 0 {
                id
            } else {
                let id = crate::class_registry::class_id_for(rt, ELEMENT_CLASS_KIND);
                if id != 0 {
                    id
                } else {
                    crate::class_registry::class_id_for(rt, NODE_CLASS_KIND)
                }
            }
        }
        _ => crate::class_registry::class_id_for(rt, NODE_CLASS_KIND),
    }
}

pub(super) unsafe fn node_opaque(
    rt: *mut sys::JSRuntime,
    this_val: sys::JSValue,
) -> *mut dom::NodeId {
    for &kind in ALL_NODE_CLASS_KINDS {
        let class_id = crate::class_registry::class_id_for(rt, kind);
        if class_id == 0 {
            continue;
        }
        let ptr = sys::JS_GetOpaque(this_val, class_id) as *mut dom::NodeId;
        if !ptr.is_null() {
            return ptr;
        }
    }
    std::ptr::null_mut()
}

pub(crate) unsafe fn node_id(
    ctx: *mut sys::JSContext,
    this_val: sys::JSValue,
) -> Option<dom::NodeId> {
    let ptr = node_opaque(sys::JS_GetRuntime(ctx), this_val);
    (!ptr.is_null()).then(|| *ptr)
}

pub(super) unsafe fn dom_opaque(ctx: *mut sys::JSContext) -> *mut dom::Dom {
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
