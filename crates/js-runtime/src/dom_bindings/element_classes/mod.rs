//! Registers the `Node`/`Element`/`HTMLElement`/HTML-subclass interface
//! hierarchy: one real `quickjs-sys` class per level (`ensure_node_class`
//! and friends), chained prototypes, and a minimal no-op constructor per
//! level exposed globally purely so `instanceof` works — real node creation
//! always goes through [`make_node_object`] with the correct `class_id`.
//!
//! Split into `classes.rs` (`ensure_*_class`), `constructors.rs` (no-op
//! `instanceof`-only constructors, `expose_constructor`, `make_node_object`),
//! and `install.rs` (`install_classes`).

mod classes;
mod constructors;
mod install;

pub(super) use constructors::{expose_constructor, make_node_object};
pub(super) use install::install_classes;
