//! Live and snapshot node collections: `NodeList`/`HTMLCollection`-shaped
//! array wrappers (`node_list`/`html_collection`) and the selector/tag/class
//! query entry points (`querySelector(All)`/`getElementsBy*`/`matches`/
//! `closest`) exposed on `Node.prototype`.
//!
//! Split into `shared.rs` (`node_array`/`collection_item`, shared by
//! both collection shapes), `html_collection.rs` (`html_collection`),
//! `node_list.rs` (`node_list`), `queries.rs` (`query_selector_all`/
//! `elements_by_tag_name`/`elements_by_class_name`), and `methods.rs`
//! (`querySelector(All)`/`getElementsBy*`/`matches`/`closest` and
//! `define_selector_methods`).

mod html_collection;
mod live;
mod methods;
mod node_list;
mod queries;
mod shared;

pub(super) use html_collection::html_collection;
pub(super) use methods::define_selector_methods;
pub(super) use node_list::node_list;
pub(super) use queries::{elements_by_class_name, elements_by_tag_name, query_selector_all};
