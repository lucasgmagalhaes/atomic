//! Stacking-context detection (`ROADMAP.md` item 28) — identifies which
//! boxes in an already-built tree get their own compositing layer.
//!
//! `establishes_stacking_context` mirrors the real CSS spec's stacking
//! context triggers, narrowed to the three this crate's `ComputedStyle`
//! actually resolves: `position != Static` with an explicit `z_index`,
//! `opacity < 1.0`, or a non-identity `transform`. `find_layer_roots` walks
//! a tree pre-order and stops descending once a box matches — a stacking
//! context nested inside another one is *not* returned as its own root; it
//! stays flattened into its nearest matching ancestor's own layer. Real
//! nested-layer compositing (each with its own independent cache) is a
//! documented scope cut, consistent with every other item this engine
//! narrows versus the full spec.

use crate::style::ComputedStyle;
use crate::tree::LayoutBox;

pub fn establishes_stacking_context(style: &ComputedStyle) -> bool {
    let positioned_with_z_index =
        style.position != crate::style::Position::Static && style.z_index.is_some();
    let translucent = style.opacity < 1.0;
    let transformed = style.transform != (0.0, 0.0);
    positioned_with_z_index || translucent || transformed
}

pub fn find_layer_roots(tree: &LayoutBox) -> Vec<&LayoutBox> {
    let mut roots = Vec::new();
    collect_layer_roots(tree, &mut roots);
    roots
}

fn collect_layer_roots<'a>(box_: &'a LayoutBox, roots: &mut Vec<&'a LayoutBox>) {
    if establishes_stacking_context(&box_.style) {
        roots.push(box_);
        return;
    }
    for child in &box_.children {
        collect_layer_roots(child, roots);
    }
}
