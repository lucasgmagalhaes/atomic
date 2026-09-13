//! `Rect`, `build_display_list`, and background/border/box-shadow
//! collection — split out from `display_list.rs`.

use layout_engine::{BorderStyle, Color, LayoutBox, Overflow};

use super::helpers::{clip_rect, paint_order, scale_alpha, tighten_clip, ClipRect};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Color,
    /// Real `border-radius` now — see `layout_engine::ComputedStyle::border_radius`'s
    /// own doc for the "one uniform value" scope cut, and `render::gpu`'s
    /// module doc for how this actually gets painted (a per-pixel signed-
    /// distance-field edge in the fragment shader, not faked geometry).
    /// `0.0` (the overwhelming majority of rects, including every clip
    /// rect and shadow rect this pass didn't touch) paints byte-identical
    /// to before this field existed.
    pub radius: f32,
}

/// Walks `box_` in paint order (parent before children, so later-painted
/// children correctly draw on top of their parent's background) and
/// collects one `Rect` per box with a non-transparent background.
/// Transparent boxes are skipped rather than emitted with a zero-alpha
/// rect — nothing downstream needs to know they exist.
pub fn build_display_list(box_: &LayoutBox) -> Vec<Rect> {
    let mut list = Vec::new();
    collect(box_, &mut list, None, 1.0, (0.0, 0.0));
    list
}

/// `translate` is the accumulated `transform` offset in effect *above*
/// `box_` (from every transformed ancestor) - `box_`'s own
/// `style.transform` is added to it before painting `box_` itself and
/// before recursing, so a transformed box's entire subtree moves with it
/// as one rigid unit, same real semantics `opacity` already has for
/// "applies to a box and composes down into its descendants" (see the
/// module doc's own opacity paragraph) - see
/// `layout_engine::ComputedStyle::transform`'s doc for the translate-only
/// scope and why this never affects layout, only where things paint.
fn collect(
    box_: &LayoutBox,
    out: &mut Vec<Rect>,
    clip: Option<ClipRect>,
    parent_opacity: f64,
    parent_translate: (f64, f64),
) {
    let opacity = parent_opacity * box_.style.opacity;
    let translate = (
        parent_translate.0 + box_.style.transform.0,
        parent_translate.1 + box_.style.transform.1,
    );
    if let Some(shadow) = box_.style.box_shadow {
        if shadow.color.a > 0 {
            push_box_shadow_rect(box_, shadow, out, clip, opacity, translate);
        }
    }
    if box_.style.background_color.a > 0 {
        let rect = Rect {
            x: (box_.dimensions.x + translate.0) as f32,
            y: (box_.dimensions.y + translate.1) as f32,
            width: box_.dimensions.width as f32,
            height: box_.dimensions.height as f32,
            color: scale_alpha(box_.style.background_color, opacity),
            radius: box_.style.border_radius as f32,
        };
        if let Some(clipped) = clip_rect(rect, clip) {
            out.push(clipped);
        }
    }
    if box_.style.border_style != BorderStyle::None && box_.style.border_color.a > 0 {
        push_border_rects(box_, out, clip, opacity, translate);
    }
    let (child_clip, child_translate) = if box_.style.overflow == Overflow::Hidden {
        (
            Some(tighten_clip(box_, clip, translate)),
            // Real per-element scroll (`ROADMAP.md` item 22): a scrolled
            // container's own background/border/shadow (already painted
            // above at the un-shifted `translate`) stay fixed; only its
            // *children* shift, by the negative of its own scroll offset
            // - matches `profile-worker`'s whole-document `y - scroll_top`
            // shift, just applied per element instead of once for the
            // page. Still clipped to the container's own (un-shifted)
            // border box via `child_clip` above.
            (
                translate.0 - box_.scroll_offset.0,
                translate.1 - box_.scroll_offset.1,
            ),
        )
    } else {
        (clip, translate)
    };
    for child in paint_order(&box_.children) {
        collect(child, out, child_clip, opacity, child_translate);
    }
}

/// Real, but flat and blur-less: paints one solid rect behind `box_`'s
/// border box, offset by `shadow.offset_x`/`offset_y` and grown on every
/// side by `shadow.spread` - see `layout_engine::BoxShadow`'s own doc for
/// why `blur-radius` isn't modeled. Pushed before the background/border
/// rects in `collect`, so real paint order (background/border on top of
/// the shadow) falls out naturally from this pipeline's existing
/// "later rects paint over earlier ones" convention - no explicit
/// z-ordering needed.
fn push_box_shadow_rect(
    box_: &LayoutBox,
    shadow: layout_engine::BoxShadow,
    out: &mut Vec<Rect>,
    clip: Option<ClipRect>,
    opacity: f64,
    translate: (f64, f64),
) {
    let d = box_.dimensions;
    let rect = Rect {
        x: (d.x + translate.0 + shadow.offset_x - shadow.spread) as f32,
        y: (d.y + translate.1 + shadow.offset_y - shadow.spread) as f32,
        width: (d.width + shadow.spread * 2.0).max(0.0) as f32,
        height: (d.height + shadow.spread * 2.0).max(0.0) as f32,
        color: scale_alpha(shadow.color, opacity),
        // Not rounded to match box_.style.border_radius this pass - a
        // shadow behind a rounded box is a real, separate scope cut left
        // for later (same "flat blur-less shadow" honesty this function's
        // own doc already states for blur-radius).
        radius: 0.0,
    };
    if let Some(clipped) = clip_rect(rect, clip) {
        out.push(clipped);
    }
}

/// Real, but flat: paints `box_`'s border as up to 4 solid-color strips
/// hugging the outer edge of its (already border-inclusive, see
/// `layout_engine::layout::layout_block`'s own doc) `dimensions`, no
/// per-side colors/styles. A side with `0` resolved width
/// (`layout_engine::ResolvedBorder`) contributes no rect, same as a
/// transparent background contributing none. The 4 strips' corners
/// slightly overlap (e.g. the top strip's own left end sits under the
/// left strip's top end) rather than being mitered - harmless since
/// every side shares one solid `border_color`, so double-painting the
/// same pixel with the same color is a no-op. `border-radius` is real
/// now too (see `Rect::radius`'s own doc), applied uniformly to every
/// strip rather than to just the two outer corners each strip actually
/// owns (a real per-corner rounding would need each strip to carry two
/// different corner radii, which this pass's SDF shader doesn't support
/// - see `render::gpu`) - an acceptable seam at typical border widths,
/// where a strip is thin enough that its "wrong" rounded corner is
/// mostly hidden under the background rect it sits on top of.
fn push_border_rects(
    box_: &LayoutBox,
    out: &mut Vec<Rect>,
    clip: Option<ClipRect>,
    opacity: f64,
    translate: (f64, f64),
) {
    let d = box_.dimensions;
    let b = box_.border;
    let color = scale_alpha(box_.style.border_color, opacity);
    let radius = box_.style.border_radius as f32;
    let x = d.x + translate.0;
    let y = d.y + translate.1;
    let mut push = |rect: Rect| {
        if let Some(clipped) = clip_rect(rect, clip) {
            out.push(clipped);
        }
    };
    if b.top > 0.0 {
        push(Rect {
            x: x as f32,
            y: y as f32,
            width: d.width as f32,
            height: b.top as f32,
            color,
            radius,
        });
    }
    if b.bottom > 0.0 {
        push(Rect {
            x: x as f32,
            y: (y + d.height - b.bottom) as f32,
            width: d.width as f32,
            height: b.bottom as f32,
            color,
            radius,
        });
    }
    if b.left > 0.0 {
        push(Rect {
            x: x as f32,
            y: y as f32,
            width: b.left as f32,
            height: d.height as f32,
            color,
            radius,
        });
    }
    if b.right > 0.0 {
        push(Rect {
            x: (x + d.width - b.right) as f32,
            y: y as f32,
            width: b.right as f32,
            height: d.height as f32,
            color,
            radius,
        });
    }
}
