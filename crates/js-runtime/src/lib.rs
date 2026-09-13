//! Safe wrapper around `quickjs-sys`. Minimal on purpose (phase 2 slice):
//! create a runtime/context and eval JS to a string. DOM↔JS bindings land
//! in a later pass once `dom` has something worth exposing.
//!
//! `Runtime`/`EvalError` live in `runtime.rs`; `Context` (split further
//! into `context/*.rs` by concern — construction, eval, limits, console/
//! navigation, viewport, render state, stylesheets) lives in `context/`.

mod abort_controller;
mod blob;
mod class_registry;
mod clipboard;
mod computed_style;
pub mod console;
mod cors;
mod crypto;
mod csp;
mod css_style;
mod cssom_stylesheet;
mod document;
mod document_cookie;
mod dom_bindings;
mod event_subclasses;
mod events;
mod fetch;
mod fetch_async;
mod form_data;
mod history;
mod host_state;
mod indexed_db_bindings;
mod js_helpers;
mod layout_measurement;
mod local_storage_bindings;
mod location;
mod mutation_observer;
mod navigator;
mod notifications;
mod page_visibility;
mod performance;
mod permissions_policy;
mod request_response;
mod screen;
mod script_limits;
mod timers;
mod trusted_types;
mod url_bindings;
mod value_bridge;
mod web_audio;
mod window;

mod context;
mod runtime;

pub use context::Context;
pub use layout_measurement::Rect;
pub use runtime::{ensure_external_class, external_class_id, EvalError, Runtime};
