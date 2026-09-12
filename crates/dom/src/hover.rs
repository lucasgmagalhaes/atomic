use crate::{Dom, NodeId};

impl Dom {
    /// Real live `:hover` state — set by a caller doing its own per-frame
    /// mouse-position hit-test (this crate has no input-event loop of its
    /// own). No-op if `id` doesn't exist. Bumps [`Dom::style_version`]
    /// (not `layout_version` — see that field's own doc) so a caller's
    /// layout/paint cache keyed on it invalidates, but never touches
    /// `mark_dirty`/`mutation_count`, same as [`Dom::focus`] already
    /// leaves both untouched.
    pub fn set_hovered(&mut self, id: NodeId) {
        if self.get(id).is_some() && self.hovered != Some(id) {
            self.hovered = Some(id);
            self.bump_style_version();
        }
    }

    /// Clears hover unconditionally, regardless of which node (if any) is
    /// currently hovered — used when the pointer moves off every element
    /// (e.g. leaves the viewport). No-op (no `style_version` bump) if
    /// nothing was hovered, mirroring [`Dom::clear_focus`]'s own
    /// "no-op on an already-clear state" convention.
    pub fn clear_hover(&mut self) {
        if self.hovered.is_some() {
            self.hovered = None;
            self.bump_style_version();
        }
    }

    /// Real read side — the node a `:hover` selector should currently
    /// match, if any. Filters out a stale id (a hovered node later
    /// removed from the tree) rather than requiring every removal path to
    /// also clear `hovered`, same reasoning [`Dom::active_element`]'s own
    /// doc gives for focus.
    pub fn hovered_element(&self) -> Option<NodeId> {
        self.hovered.filter(|&id| self.get(id).is_some())
    }
}
