//! Real coordinate-to-DOM-node hit testing over an already-laid-out
//! `LayoutBox` tree — the primitive real mouse input needs (see
//! `mockup/rendering-engine-gaps.md`'s "input real" gap: previously a
//! click on a rendered page had no way to reach a DOM node at all, only
//! `#id`-selector-driven automation could).
//!
//! Purely additive: reads `LayoutBox::dimensions` (already real, already
//! computed by `layout::layout_block`/`flex::layout_flex_children`), adds
//! no new fields and changes no existing layout code.
//!
//! Scope matches this engine's existing layout gaps (see the gap spec):
//! no `overflow`/clipping and no `z-index`/stacking context exist yet, so
//! "topmost box at this point" is approximated as "last child in tree
//! order whose box contains the point" (painter's-algorithm order,
//! matching how `render::display_list` already paints) rather than a real
//! stacking-context-aware hit test — correct for everything this engine
//! can currently lay out, since nothing overlaps except through this
//! paint-order convention anyway.
use crate::tree::{Dimensions, LayoutBox};
use dom::NodeId;

fn contains(d: &Dimensions, x: f64, y: f64) -> bool {
    x >= d.x && x < d.x + d.width && y >= d.y && y < d.y + d.height
}

/// Finds the deepest (most specific) box under `(x, y)`, walking children
/// in reverse tree order so a later (visually "on top") sibling wins over
/// an earlier one when both happen to contain the point. Returns the
/// real `dom::NodeId` a caller can then `dispatchEvent`/focus/read
/// attributes from — same `NodeId` type every other real DOM operation in
/// this workspace already uses, not a synthetic layout-only handle.
///
/// `None` if `(x, y)` falls outside `root` entirely (e.g. a click below
/// the tallest content, in empty viewport space).
pub fn hit_test(root: &LayoutBox, x: f64, y: f64) -> Option<NodeId> {
    if !contains(&root.dimensions, x, y) {
        return None;
    }
    for child in root.children.iter().rev() {
        if let Some(hit) = hit_test(child, x, y) {
            return Some(hit);
        }
    }
    Some(root.node)
}
