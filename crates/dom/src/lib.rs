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
mod hover;
mod mutation;
mod query;
mod serialize;

/// Classifies which subsystems need recomputation after a DOM mutation.
/// Each flag corresponds to a downstream consumer that can skip work when
/// its flag is clear — the core mechanism behind lazy invalidation
/// (see `spec/architecture/primitives.md` §3.2–3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DirtyFlags(u8);

impl DirtyFlags {
    pub const EMPTY: Self = Self(0);
    /// DOM tree structure changed (nodes added/removed/reordered).
    pub const DOM: Self = Self(1 << 0);
    /// Live collections (HTMLCollection, NodeList) need recomputation.
    pub const COLLECTION: Self = Self(1 << 1);
    /// Selector matching needs recomputation.
    pub const SELECTORS: Self = Self(1 << 2);
    /// Style cascade needs recomputation.
    pub const STYLE: Self = Self(1 << 3);
    /// Layout needs recomputation.
    pub const LAYOUT: Self = Self(1 << 4);
    /// Paint / display list needs recomputation.
    pub const PAINT: Self = Self(1 << 5);
    /// Accessibility tree needs recomputation.
    pub const A11Y: Self = Self(1 << 6);

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl std::ops::BitOr for DirtyFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for DirtyFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    index: usize,
    generation: u32,
}

#[derive(Debug, Clone)]
pub enum NodeData {
    Document,
    Element {
        tag: atoms::Atom,
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
        /// Real, independent `.checked` — mirrors `value`'s own doc: an
        /// independently-mutable live property, not just a reflection of
        /// the `checked` content attribute (that's `.defaultChecked`, see
        /// `js_runtime::dom_bindings::content`). Always mirrored in from
        /// `set_attribute`/`remove_attribute` when the `checked` attribute
        /// changes, same "no dirty-value-flag tracking, matches this
        /// engine's only real caller (html5ever's tree builder, before any
        /// script runs)" simplification `value`'s own mirroring already
        /// documents. `false` — not `Option`, unlike `value` — since a
        /// boolean attribute's absence unambiguously means `false`, with
        /// no `<textarea>`-shaped fallback to disambiguate from "never
        /// set" the way `value` needs.
        checked: bool,
        /// Real, independent text-cursor/selection range for `.value`
        /// (`selectionStart`/`selectionEnd`/`selectionDirection`, the
        /// `setSelectionRange` method's backing state) — `(0, 0, "none")`
        /// until either a page script calls `setSelectionRange` or
        /// `Dom::set_value` moves the cursor to the end of the new text
        /// (matching a real `<input>`/`<textarea>` resetting its caret on
        /// a fresh `.value =`). Present on every `Element` for the same
        /// reason `value`/`checked` are: one generic `Node` JS class, not
        /// a typed `HTMLInputElement`/`HTMLTextAreaElement` hierarchy.
        selection: (usize, usize, String),
        /// Real, independent `.selected` for `<option>` — mirrors
        /// `checked`'s own doc exactly: an independently-mutable live
        /// property, not just a reflection of the `selected` content
        /// attribute (that's `.defaultSelected`, see
        /// `js_runtime::dom_bindings::content`). Always mirrored in from
        /// `set_attribute`/`remove_attribute` when the `selected`
        /// attribute changes, same as `checked`.
        selected: bool,
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
    /// Real live `:hover` state — the one node currently hovered, or
    /// `None`. See [`Dom::set_hovered`]/[`Dom::clear_hover`]/
    /// [`Dom::hovered_element`]. Mirrors `focused`'s shape exactly; a
    /// caller (e.g. a native embedding's per-frame mouse-move hit-test)
    /// decides *when* this changes, same as focus.
    hovered: Option<NodeId>,
    /// Bumped whenever `hovered`/`focused` change (see
    /// [`Dom::style_version`]) — a hover/focus change can affect which
    /// CSS rules match (`:hover`/`:focus`) without ever affecting layout,
    /// so it's a separate counter from `layout_version`: a caller with a
    /// layout cache keyed on `layout_version` alone would otherwise never
    /// notice its cached box tree's baked-in cascaded styles went stale.
    style_version: u64,
    /// Backward-compatible monotonic mutation counter — bumped by every
    /// structural/content mutation, same as before. Kept for callers that
    /// still use `mutation_count()` (e.g. `profile-worker`'s layout cache).
    mutations: u64,
    /// Classified dirty flags — set by each mutation, drained by
    /// consumers via [`Dom::drain_dirty`]. Lets each subsystem
    /// (collections, selectors, style, layout, paint, a11y) skip work
    /// when its flag is clear, rather than invalidating everything.
    dirty: DirtyFlags,
    /// Monotonically increasing counter bumped only when the LAYOUT
    /// flag is set by [`Dom::mark_dirty`]. Lets callers that only need
    /// "did layout change?" (e.g. `profile-worker`'s layout cache) do
    /// a cheap integer comparison without touching the full dirty-flags
    /// drain API — which requires `&mut self` and isn't available from
    /// an immutable borrow of `Dom`.
    layout_version: u64,
    /// Real `MutationObserver` record queue (`ROADMAP.md` item 40):
    /// appended by every tree-shape mutation (`append_child`/
    /// `insert_before`/`remove_from_parent`/`remove`/`replace_child`) and
    /// attribute mutation (`set_attribute`/`remove_attribute`), drained by
    /// [`Dom::take_mutation_records`]. A caller (`js_runtime`'s
    /// `mutation_observer` module) matches each record's `target` against
    /// registered observers and delivers the ones that match.
    mutation_records: Vec<MutationRecord>,
    /// Real dirty-style-propagation queue (`ROADMAP.md` item 11): appended
    /// by every mutation that can affect cascade results, drained by
    /// [`Dom::drain_style_invalidations`]. Unlike `dirty`/`style_version`
    /// (a single global "something changed" signal), each entry names
    /// *which* node needs re-cascading and whether its descendants do too
    /// — a consumer (`layout-engine`'s future incremental style pass, see
    /// ROADMAP item 27) can restyle only the named subtrees instead of the
    /// whole document. Same drain-and-reset shape as `mutation_records`.
    style_invalidations: Vec<StyleInvalidation>,
}

/// One entry in [`Dom::drain_style_invalidations`]'s queue.
///
/// `subtree: true` means `root` and every descendant may need
/// re-cascading — a structural change (a subtree was added/moved, whose
/// own elements have never been cascaded against this tree's rules) or an
/// attribute mutation (`class`/`style`/anything else — this crate can't
/// know which specific properties an arbitrary attribute change affects,
/// so it's conservative here the same way `attributes::ATTR_DIRTY`
/// already is, and inherited properties mean a changed value can affect
/// descendants regardless of which attribute changed).
///
/// `subtree: false` means only `root` itself needs re-cascading — a
/// `:hover`/`:focus` state change, which affects which rule matches the
/// exact node whose state changed but not, in this engine's scope, its
/// descendants (a real page can write `.foo:hover .bar { ... }`, which
/// this narrower classification misses — documented scope cut, not a
/// silent bug: the *global* `style_version` counter still bumps
/// alongside every entry pushed here, so any consumer that isn't ready to
/// trust per-subtree scoping yet can keep comparing that instead, same
/// fallback `LayoutCache`/`PaintCache` already use).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleInvalidation {
    pub root: NodeId,
    pub subtree: bool,
}

/// One entry in [`Dom::take_mutation_records`]'s queue — either a
/// `childList` change (direct children of `target` added/removed) or an
/// `attributes` change (one attribute on `target` set/removed). Scoped
/// down from the full spec: no `subtree` observation (a record's `target`
/// is always the exact node whose own children/attributes changed, never
/// a descendant several levels down) and no `characterData` records
/// (`Text`/`Comment` `.data` mutations don't push one) — real, narrower
/// coverage for the two record kinds real pages overwhelmingly use.
#[derive(Debug, Clone)]
pub struct MutationRecord {
    pub target: NodeId,
    pub kind: MutationRecordKind,
}

#[derive(Debug, Clone)]
pub enum MutationRecordKind {
    ChildList {
        added: Vec<NodeId>,
        removed: Vec<NodeId>,
    },
    Attributes {
        name: String,
        /// The attribute's value immediately before this change (`None`
        /// if it didn't exist yet) — always captured; a plain `HashMap`
        /// lookup is cheap enough that gating it behind whether some
        /// observer asked for `attributeOldValue` isn't worth the added
        /// plumbing (per this crate's own YAGNI convention).
        old_value: Option<String>,
    },
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
            hovered: None,
            style_version: 0,
            mutations: 0,
            dirty: DirtyFlags::EMPTY,
            layout_version: 0,
            mutation_records: Vec::new(),
            style_invalidations: Vec::new(),
        }
    }

    /// Accumulate dirty flags — called by every mutation method.
    /// Also bumps the backward-compatible `mutations` counter and
    /// `layout_version` when the LAYOUT flag is set.
    fn mark_dirty(&mut self, flags: DirtyFlags) {
        self.mutations = self.mutations.wrapping_add(1);
        if flags.contains(DirtyFlags::LAYOUT) {
            self.layout_version = self.layout_version.wrapping_add(1);
        }
        self.dirty |= flags;
    }

    /// Real "has `:hover`/`:focus`-matchable state changed?" signal — see
    /// [`Dom::style_version`]'s field doc for why this is separate from
    /// `layout_version`. Called by [`hover::set_hovered`]/[`clear_hover`]
    /// and [`focus`]/[`blur`]/[`clear_focus`], never by a structural
    /// mutation (those already bump `layout_version` via `mark_dirty`).
    pub(crate) fn bump_style_version(&mut self) {
        self.style_version = self.style_version.wrapping_add(1);
    }

    /// Current value of the counter [`bump_style_version`] increments —
    /// a caller with a layout/paint cache keyed on `layout_version` alone
    /// (e.g. a chrome-engine embedding's box-tree cache) should also key
    /// on this, so a hover/focus change that doesn't touch layout still
    /// invalidates a cache whose cascaded styles depend on it.
    pub fn style_version(&self) -> u64 {
        self.style_version
    }

    /// Records that `root` (and, if `subtree`, its descendants) needs
    /// re-cascading — see [`StyleInvalidation`]'s own doc for the
    /// `subtree` distinction. Deliberately does *not* touch
    /// [`Dom::style_version`]/`layout_version` — those two counters keep
    /// their existing, narrower meaning (`style_version`: a `:hover`/
    /// `:focus` state change; `layout_version`: a `LAYOUT`-flagged
    /// mutation, via `mark_dirty`) and callers already keyed on them must
    /// not see extra bumps from every attribute/structural mutation this
    /// queue also now records.
    pub(crate) fn push_style_invalidation(&mut self, root: NodeId, subtree: bool) {
        self.style_invalidations
            .push(StyleInvalidation { root, subtree });
    }
}
