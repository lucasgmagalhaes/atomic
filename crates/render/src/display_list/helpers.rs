//! Shared paint-order sorting, alpha scaling, and clip-region helpers —
//! split out from `display_list.rs`.

use layout_engine::{LayoutBox, Position};

/// Real paint-order sort for one stacking context's immediate `children` —
/// see `layout_engine::ComputedStyle::z_index`'s own doc for what
/// "establishes a stacking context" means here (`position != Static &&
/// z_index.is_some()` — an explicit integer, not `auto`). Three
/// stable-sorted buckets, painted in this order: stacking-context children
/// with a negative z-index (most negative first, so it paints furthest
/// back), then everything else in tree order (non-positioned content and
/// `z-index: auto` positioned content aren't distinguished from each other
/// this pass — real CSS gives the latter its own later paint step even
/// without an explicit z-index; a documented, narrower scope cut, same
/// spirit as this crate's other "real but not spec-exact" simplifications),
/// then stacking-context children with a non-negative z-index (least
/// positive first, so the highest value paints last/on top). Each
/// `collect_*` walk in this module calls this once per level instead of
/// iterating `box_.children` directly — the sort is inherently recursive
/// through the walk itself (a nested stacking context's own children get
/// reordered independently, inside its own recursive call), so nothing
/// deeper than "sort my immediate children" is needed here.
pub(super) fn paint_order(children: &[LayoutBox]) -> Vec<&LayoutBox> {
    let establishes_stacking_context =
        |b: &LayoutBox| b.style.position != Position::Static && b.style.z_index.is_some();
    let mut behind = Vec::new();
    let mut normal = Vec::new();
    let mut front = Vec::new();
    for child in children {
        if establishes_stacking_context(child) {
            if child.style.z_index.unwrap() < 0 {
                behind.push(child);
            } else {
                front.push(child);
            }
        } else {
            normal.push(child);
        }
    }
    behind.sort_by_key(|c| c.style.z_index.unwrap());
    front.sort_by_key(|c| c.style.z_index.unwrap());
    behind.into_iter().chain(normal).chain(front).collect()
}

/// Scales `color`'s alpha channel by `opacity` (`1.0` = unchanged) -
/// shared by every paint primitive's opacity handling in this module.
pub(super) fn scale_alpha(color: layout_engine::Color, opacity: f64) -> layout_engine::Color {
    layout_engine::Color {
        a: (color.a as f64 * opacity).round().clamp(0.0, 255.0) as u8,
        ..color
    }
}

/// An axis-aligned clip region in absolute page coordinates - the
/// intersection of every `overflow`-clipping ancestor's own border box
/// seen so far on a walk down the tree. Kept separate from `Rect`
/// (which also carries a `color`) since a clip region is pure geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ClipRect {
    pub(super) fn intersect(&self, other: &ClipRect) -> ClipRect {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.width).min(other.x + other.width);
        let y1 = (self.y + self.height).min(other.y + other.height);
        ClipRect {
            x: x0,
            y: y0,
            width: (x1 - x0).max(0.0),
            height: (y1 - y0).max(0.0),
        }
    }
}

/// Intersects `rect` against `clip` (`None` = unclipped, passes through
/// unchanged). `None` if the two don't overlap at all - the caller drops
/// the rect entirely, same as an already-transparent box contributing
/// nothing.
pub(super) fn clip_rect(rect: super::Rect, clip: Option<ClipRect>) -> Option<super::Rect> {
    let Some(clip) = clip else { return Some(rect) };
    let x0 = rect.x.max(clip.x);
    let y0 = rect.y.max(clip.y);
    let x1 = (rect.x + rect.width).min(clip.x + clip.width);
    let y1 = (rect.y + rect.height).min(clip.y + clip.height);
    if x1 <= x0 || y1 <= y0 {
        None
    } else {
        Some(super::Rect {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
            color: rect.color,
            radius: rect.radius,
            // Real, but not spec-exact: a clipped gradient rect's gradient
            // re-stretches to whatever sub-rect actually survives clipping
            // rather than staying anchored to the original box's own
            // extent (`render::gpu::shader::rect_to_vertices` computes the
            // gradient purely from a `Rect`'s own current x/y/width/height
            // - it has no memory of a pre-clip size to anchor against).
            gradient: rect.gradient,
            fixed: rect.fixed,
        })
    }
}

/// `box_`'s own border box as a [`ClipRect`], intersected with whatever
/// clip was already in effect from an ancestor - `None` in, `None` out
/// only if `box_` itself doesn't clip either (checked by the caller
/// before calling this; this function always produces a real bound).
/// `translate` is the accumulated `transform` offset in effect at `box_`
/// itself (see `collect`'s own doc) - the clip region moves with the box
/// it belongs to, same as every other paint primitive.
pub(super) fn tighten_clip(
    box_: &LayoutBox,
    clip: Option<ClipRect>,
    translate: (f64, f64),
) -> ClipRect {
    let own = ClipRect {
        x: (box_.dimensions.x + translate.0) as f32,
        y: (box_.dimensions.y + translate.1) as f32,
        width: box_.dimensions.width as f32,
        height: box_.dimensions.height as f32,
    };
    match clip {
        Some(c) => c.intersect(&own),
        None => own,
    }
}
