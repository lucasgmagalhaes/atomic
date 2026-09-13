//! Compositing layer groundwork (`ROADMAP.md` item 28;
//! `.claude/plans/architecture-p5-foundations.plan.md` Stage 5).
//!
//! Scope cut, deliberate: this only introduces the [`Layer`] type and a
//! per-layer paint cache key — it does **not** implement real GPU layer
//! compositing (texture atlasing, layer promotion heuristics, actual
//! multi-pass GPU rendering). Today `profile-worker`'s own `PaintCache`
//! (item 13) caches one whole-frame pixel buffer keyed on a version
//! tuple; [`Layer`] generalizes that same cache shape to be keyed per
//! stacking-context-establishing element instead of per-page, so a
//! mutation inside one layer's subtree doesn't need to invalidate layers
//! that didn't change — the actual point of compositing. Wiring this
//! into `profile-worker`'s real render pipeline (replacing its single
//! `PaintCache` with a set of these, keyed by which stacking contexts
//! exist) is item 28's own remaining, larger work — items 24/25
//! (stacking-context/z-index), already P3, are this stage's real input.

use dom::NodeId;

/// One compositing layer: the pixel buffer produced by compositing
/// everything rooted at `stacking_context_root`, valid only while `key`
/// still matches the inputs it was built from.
pub struct Layer {
    pub stacking_context_root: NodeId,
    pub key: LayerCacheKey,
    pub pixels: Vec<u8>,
}

/// The version tuple a [`Layer`]'s cached `pixels` stay valid against —
/// the same fields `profile-worker`'s `PaintCache` already keys on
/// (`width`/`height`/`layout_ver`/`style_ver`/`adopted_version`), since a
/// layer's own subtree is still driven by the same document-wide
/// versions until `dom`'s per-subtree `StyleInvalidation` (item 11,
/// Stage 2) is wired through here too — a further refinement this stage
/// doesn't attempt. Even at this coarser granularity, keying per layer
/// is still strictly more precise than one whole-frame cache: layers
/// with a different `stacking_context_root` are independent entries a
/// caller can compare and reuse separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerCacheKey {
    pub width: u32,
    pub height: u32,
    pub layout_ver: u64,
    pub style_ver: u64,
    pub adopted_version: u64,
}

impl Layer {
    /// A fresh, empty layer for `stacking_context_root` with no cached
    /// pixels yet — `key` is a sentinel no real key ever equals
    /// (`u64::MAX`/`u32::MAX` fields), guaranteeing the first
    /// [`Layer::is_valid`] check misses so a caller always does a real
    /// composite before ever reusing stale (empty) pixels.
    pub fn new(stacking_context_root: NodeId) -> Self {
        Layer {
            stacking_context_root,
            key: LayerCacheKey {
                width: u32::MAX,
                height: u32::MAX,
                layout_ver: u64::MAX,
                style_ver: u64::MAX,
                adopted_version: u64::MAX,
            },
            pixels: Vec::new(),
        }
    }

    /// Whether this layer's cached `pixels` are still valid against
    /// `key` — the same "compare the key, skip recompute on a hit" check
    /// `PaintCache`'s own caller (`Page::render`) already does inline;
    /// factored out here since a caller managing several layers needs to
    /// run it per layer.
    pub fn is_valid(&self, key: LayerCacheKey) -> bool {
        self.key == key
    }

    /// Replaces this layer's cached pixels and key after a real
    /// composite — called by a caller (a future `profile-worker`
    /// integration, item 28's own remaining work) once it has actually
    /// repainted this layer's subtree.
    pub fn store(&mut self, key: LayerCacheKey, pixels: Vec<u8>) {
        self.key = key;
        self.pixels = pixels;
    }
}
