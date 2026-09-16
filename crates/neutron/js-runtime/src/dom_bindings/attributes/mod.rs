//! Direct attribute reflection: the fixed `id`/`className`/`name`/`type`/
//! `href`/`checked`/`disabled`/`selected`/`htmlFor` accessor pairs on
//! `Node.prototype`, plus the real `attributes`/`NamedNodeMap`-shaped
//! collection (`ATTRS_OBJECTS`).
//!
//! Split into `reflected.rs` (the fixed accessor pairs) and
//! `collection.rs` (the `attributes`/`NamedNodeMap` collection) — this
//! file just re-exports their public entry points.

mod collection;
mod reflected;

pub(super) use collection::{cleanup, define_attributes_collection};
pub(super) use reflected::define_attribute_properties;
