//! Bounded DOM-style event dispatch for `Node` objects.
//!
//! Event state is native structured data. User code is invoked only as a
//! QuickJS function; no supplied string is parsed or evaluated by this
//! layer.
//!
//! Split into one file per cohesive responsibility (SRP), same convention
//! this workspace's other large-file splits use:
//! - [`util`]: shared string/error helpers and the getter-binding helper.
//! - [`event_class`]: the `Event` native class itself — opaque
//!   `EventState`, its property getters/`preventDefault`/
//!   `stopPropagation`, the JS constructor, `register`, and the
//!   [`event_class::make_event`]/[`create_event`] builders.
//! - [`listeners`]: `addEventListener`/`removeEventListener` and the
//!   per-`(node, type)` listener storage (`{callback, capture, once,
//!   passive}` records as plain JS values, no extra native class).
//! - [`dispatch`]: the three-phase (capture/target/bubble) DOM traversal,
//!   the nested-dispatch depth guard, and `dispatchEvent`/`EventTarget`
//!   itself (both the real `Node`-tree-walking form and the single-
//!   target-phase form non-`Node` targets like `window`/`document` use).

mod dispatch;
mod event_class;
mod listeners;
mod util;

pub(crate) use dispatch::{
  define_event_target, define_simple_event_target, dispatch, dispatch_event_object, dispatch_simple,
};
pub(crate) use event_class::{create_event, register};
