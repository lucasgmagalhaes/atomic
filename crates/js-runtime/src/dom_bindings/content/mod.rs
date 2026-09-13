//! Text/value/HTML content accessors on `Node.prototype`: `textContent`,
//! `value`, `nodeValue`, and the three real HTML injection sinks
//! (`innerHTML`/`outerHTML`/`insertAdjacentHTML`), all gated through
//! `crate::trusted_types` where applicable.
//!
//! Split into `text_value.rs` (`textContent`/`value`/`nodeValue`),
//! `default_attrs.rs` (`defaultValue`/`defaultChecked`/`defaultSelected`),
//! `selection.rs` (`selectionStart`/`selectionEnd`/`selectionDirection`/
//! `setSelectionRange`), and `inner_outer_html.rs` (`innerHTML`/
//! `outerHTML`/`insertAdjacentHTML`) — this file just re-exports every
//! `define_*` entry point at the same `content::` path callers already use.

mod default_attrs;
mod inner_outer_html;
mod selection;
mod text_value;

pub(super) use default_attrs::{
    define_default_checked, define_default_selected, define_default_value,
};
pub(super) use inner_outer_html::define_inner_outer_html;
pub(super) use selection::define_selection_properties;
pub(super) use text_value::{define_node_value, define_text_content, define_value};

/// Only `node_insert_adjacent_html` needs to be wired up on
/// `Node.prototype` — done by `define_mutation_methods` in `mutation.rs`
/// instead of here, since it's grouped with the other `Node.prototype`
/// *methods* (as opposed to accessor pairs) there.
pub(crate) use inner_outer_html::node_insert_adjacent_html as insert_adjacent_html;
