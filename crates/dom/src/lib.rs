//! Arena-based DOM tree. Split into one file per cohesive responsibility
//! group (SRP), same convention `js-runtime`'s `dom_bindings` split uses:
//! - [`mutation`]: node creation and tree-shape mutation (`create_*`,
//!   `append_child`, `insert_before`, `remove*`, `replace_child`).
//! - [`attributes`]: attribute/text/value read+write (`set_attribute`,
//!   `text_content`, `value`, `node_value`, `find_by_id`).
//! - [`focus`]: `document.activeElement`/tab-order state.
//! - [`clone`]: `cloneNode`/cross-arena `adopt`/`normalize`.
//! - [`serialize`]: `innerHTML`/`outerHTML` read side.
//! - [`query`]: slot lookup plus read-only tree-position queries
//!   (`contains`, `isConnected`, `compareDocumentPosition`).
//!
//! All fields on [`Dom`] stay private to this crate and are reached
//! directly from every submodule above (they're descendants of this
//! module), matching how `dom_bindings`'s own submodules reach its
//! thread-local registries.

use std::collections::HashMap;

mod attributes;
mod clone;
mod focus;
mod mutation;
mod query;
mod serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
  index: usize,
  generation: u32,
}

#[derive(Debug, Clone)]
pub enum NodeData {
  Document,
  Element {
    tag: String,
    attributes: HashMap<String, String>,
    /// Real, independently-mutable form value — `Some` once either the
    /// `value` attribute or `Dom::set_value`/JS `.value =` has touched
    /// it, `None` beforehand (in which case [`Dom::value`] falls back
    /// to the element's own rules, see that method's doc). Present on
    /// every `Element`, not just `<input>`/`<textarea>` — this engine
    /// has one generic `Node` JS class, not a per-tag
    /// `HTMLInputElement`/`HTMLTextAreaElement` hierarchy, so `.value`
    /// is a documented deviation exposed generically rather than typed
    /// per element.
    value: Option<String>,
  },
  Text(String),
  Comment(String),
  DocumentFragment,
}

#[derive(Debug, Clone)]
pub struct Node {
  pub id: NodeId,
  pub data: NodeData,
  pub parent: Option<NodeId>,
  pub children: Vec<NodeId>,
}

struct Slot {
  generation: u32,
  node: Option<Node>,
}

pub struct Dom {
  slots: Vec<Slot>,
  free: Vec<usize>,
  root: NodeId,
  /// Real `document.activeElement` state — the one node currently
  /// focused, or `None`. See [`Dom::focus`]/[`Dom::blur`]/
  /// [`Dom::clear_focus`]/[`Dom::active_element`]. No tab order/
  /// `tabindex` model yet: this only tracks *which* node is focused, not
  /// how focus moves between them.
  focused: Option<NodeId>,
  /// `.value` at the moment [`Dom::focus`] was called on `focused`, kept
  /// only so [`Dom::blur`]/[`Dom::clear_focus`] can tell a caller whether
  /// a real `"change"` event should fire (real DOM semantics: `change`
  /// fires on commit — blur after the value actually moved — not on
  /// every keystroke, unlike `input`).
  focused_value_snapshot: Option<String>,
  /// Bumped by every structural/content mutation (`append_child`,
  /// `insert_before`, `remove_from_parent`, `remove`, `set_attribute`,
  /// `remove_attribute`, `append_text`, `set_text_content`, `set_value`)
  /// — deliberately *not* by `focus`/`blur`/`clear_focus`, which don't
  /// affect anything `layout-engine` computes (no focus-ring rendering
  /// exists). Lets a caller (`profile-worker`'s `Page::layout`) detect
  /// "has anything relevant to layout changed since I last laid this
  /// page out" with one cheap integer comparison instead of diffing the
  /// whole tree — the real backing signal for incremental layout
  /// invalidation. Wrapping add is fine: a `u64` wrapping around during
  /// one process's lifetime is not a real scenario, and even a wrapped
  /// value still changes on every mutation, which is all a caller
  /// actually checks (equality, not ordering).
  mutations: u64,
}

impl Default for Dom {
  fn default() -> Self {
    Self::new()
  }
}

impl Dom {
  pub fn new() -> Self {
    let root_id = NodeId {
      index: 0,
      generation: 0,
    };
    let root = Node {
      id: root_id,
      data: NodeData::Document,
      parent: None,
      children: vec![],
    };
    Dom {
      slots: vec![Slot {
        generation: 0,
        node: Some(root),
      }],
      free: vec![],
      root: root_id,
      focused: None,
      focused_value_snapshot: None,
      mutations: 0,
    }
  }
}
