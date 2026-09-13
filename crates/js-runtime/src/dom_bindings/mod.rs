//! DOM↔JS bindings: a `Node` JS class wrapping a boxed `dom::NodeId` as its
//! opaque data, plus `document.getElementById(id)` and the `textContent`
//! accessor on `Node.prototype`. Backed by a `dom::Dom` stashed in the
//! context's opaque slot (see `Context::with_dom`).
//!
//! Split into one file per cohesive responsibility group (SRP):
//! - [`node_registry`]: the real per-`dom::NodeId` object identity cache
//!   (`node_object`, `NODE_OBJECTS`) plus the opaque-slot readers
//!   (`node_id`/`dom_opaque`) and class-kind lookup every other submodule
//!   here builds on.
//! - [`util`]: shared string/error helpers and length-bound constants.
//! - [`selectors`]: CSS selector matching against the live DOM.
//! - [`collections`]: `NodeList`/`HTMLCollection`-shaped wrappers and the
//!   `querySelector`/`getElementsBy*`/`matches`/`closest` entry points.
//! - [`attributes`]: direct attribute reflection (`id`/`className`/...)
//!   plus the `attributes`/`NamedNodeMap` collection.
//! - [`class_list`]: `Element.classList`.
//! - [`dataset`]: `HTMLElement.dataset`.
//! - [`navigation`]: read-only tree-navigation accessors.
//! - [`content`]: `textContent`/`value`/`nodeValue`/`innerHTML`/`outerHTML`/
//!   `insertAdjacentHTML`.
//! - [`mutation`]: tree-mutating methods (`appendChild`, `setAttribute`, ...).
//! - [`forms`]: `HTMLFormElement`/`HTMLSelectElement` properties plus
//!   constraint validation and `labels`.
//! - [`scroll_focus`]: scroll stubs and real `focus()`/`blur()`.
//! - [`element_classes`]: the `Node`/`Element`/`HTMLElement`/HTML-subclass
//!   `quickjs-sys` class hierarchy itself.
//! - [`document`]: the global `document` object plus `DOMParser`/
//!   `XMLSerializer`.
//!
//! Real per-`dom::NodeId` object identity now (`node_registry::node_object`)
//! — a genuine bug this fixes, not a new feature: `document.getElementById(id)`
//! used to build a brand-new `Node` JS object on every single call, so
//! `el.addEventListener(...)` followed by a *separate*
//! `document.getElementById(id)` call later (any real page calling it
//! twice — extremely common; a listener attached at page-load time and
//! dispatched from a later, separate `eval()`, e.g. a real coordinate click
//! routed through `profile-worker`) landed the listener on a wrapper object
//! that was never seen again, since `events.rs`'s `__listeners` storage
//! lives as an own property on the wrapper instance, not keyed by `NodeId`
//! itself. Caching one JS object per real `NodeId` (thread-local, keyed by
//! `JSContext` pointer then `NodeId` — same convention as `timers`/
//! `fetch_async`'s own registries) gives every lookup of the same DOM node
//! the same JS object identity, so its properties (listeners included)
//! actually persist the way a real `Node`'s object identity does.

use quickjs_sys as sys;

mod attributes;
mod class_list;
mod collections;
mod content;
mod dataset;
mod document;
mod document_creation;
mod document_parsers;
mod document_properties;
mod document_properties_define;
mod document_query;
mod element_classes;
mod forms;
mod mutation;
mod navigation;
mod node_registry;
mod scroll_focus;
mod select;
mod selectors;
mod util;
mod validity;

pub(crate) use forms::run_default_click_action;
pub(crate) use node_registry::{cleanup, node_class_id_for, node_id, node_object, parent_node_id};

/// Registers the `Node`/`Element`/`HTMLElement`/HTML-subclass interface
/// hierarchy (`element_classes::install_classes`) and the global `document`
/// object plus `DOMParser`/`XMLSerializer` (`document::install`).
pub(crate) unsafe fn register(ctx: *mut sys::JSContext) {
    element_classes::install_classes(ctx);
    document::install(ctx);
}
