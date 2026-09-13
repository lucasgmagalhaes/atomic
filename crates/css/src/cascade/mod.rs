//! Selector matching + cascade ordering. Decoupled from `dom` on purpose —
//! takes a plain ancestor-chain snapshot rather than a live tree, so this
//! crate doesn't need to depend on `dom`'s node representation. Whatever
//! wires CSS into layout builds the snapshot from a real `dom::Dom` walk.
//!
//! [`ElementSnapshot`] also carries `attributes` and sibling-position data
//! (`preceding_siblings`/`has_following_sibling`) now, for attribute
//! selectors, sibling combinators (`+`/`~`), and structural pseudo-classes
//! (`:first-child`/`:last-child`/`:nth-child`) — a caller that doesn't
//! populate them (they default to empty/`false` via `Default`) simply
//! never matches those selector kinds, rather than panicking; that's a
//! real, honest limitation for a caller that hasn't been updated yet, not
//! fake behavior.
//!
//! Split into `snapshot.rs` (`ElementSnapshot`/`selector_matches`),
//! `matching.rs` (the unindexed `matching_declarations` path), and
//! `index.rs` (the `SelectorIndex`-accelerated path).

mod index;
mod matching;
mod snapshot;

pub use index::{build_selector_index, matching_declarations_indexed, SelectorIndex};
pub use matching::{matching_declarations, MatchedDeclarations};
pub use snapshot::{selector_matches, ElementSnapshot};
