use crate::{DirtyFlags, Dom, NodeId};

impl Dom {
    /// Real per-element scroll offset (`ROADMAP.md` item 22) — `(0.0, 0.0)`
    /// for any node never scrolled, matching a freshly laid-out element's
    /// real state.
    pub fn element_scroll_offset(&self, id: NodeId) -> (f64, f64) {
        self.element_scroll.get(&id).copied().unwrap_or((0.0, 0.0))
    }

    /// Sets `id`'s real scroll offset. No-op if `id` doesn't exist. Never
    /// touches layout — boxes keep their real positions; only which slice
    /// of an overflowing container paints changes — or selector/cascade
    /// re-matching (`push_style_invalidation`; this engine has no
    /// `:hover`/`:focus`-shaped pseudo-class for scroll position). Still
    /// bumps `style_version()`, the same real invalidation signal
    /// `Dom::focus`/`hover::set_hovered` already use: a host's whole-frame
    /// `PaintCache` is keyed on `style_version`, not `DirtyFlags::PAINT`
    /// (see that cache's own doc), so scrolling would otherwise silently
    /// keep serving a stale pre-scroll frame forever - the exact bug this
    /// counter already exists to prevent for `:hover`/`:focus`.
    pub fn set_element_scroll_offset(&mut self, id: NodeId, x: f64, y: f64) {
        if self.get(id).is_none() {
            return;
        }
        self.element_scroll.insert(id, (x, y));
        self.mark_dirty(DirtyFlags::PAINT);
        self.bump_style_version();
    }
}
