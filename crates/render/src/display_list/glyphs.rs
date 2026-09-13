//! `ClippedGlyph`/`build_glyph_list` — split out from `display_list.rs`.

use layout_engine::{LayoutBox, Overflow, PositionedGlyph};

use super::helpers::{paint_order, tighten_clip, ClipRect};

/// A shaped glyph plus the clip region in effect where it's painted
/// (`None` when no `overflow`-clipping ancestor applies) and the real
/// accumulated `opacity` in effect there (`1.0` = fully opaque, no
/// ancestor set `opacity` below `1.0`). A thin wrapper rather than fields
/// on `layout_engine::PositionedGlyph` itself: that type is built one box
/// at a time deep inside `layout_engine`'s own text shaping, with no
/// notion of an ancestor's clip region or opacity - only this crate's
/// tree walk (`collect_glyphs`) accumulates those. `crate::text::composite_glyphs`
/// intersects `clip` against its own per-pixel destination bounds check
/// and multiplies each pixel's alpha by `opacity`, both real and exact
/// since it already iterates pixel-by-pixel.
#[derive(Debug, Clone, Copy)]
pub struct ClippedGlyph {
    pub glyph: PositionedGlyph,
    pub clip: Option<ClipRect>,
    pub opacity: f64,
}

/// Same paint-order walk as [`super::build_display_list`], but collects
/// glyphs instead of rects. `PositionedGlyph::x`/`y` are relative to the
/// text box's own top-left (see `layout_engine::text`'s docs) - this
/// shifts them to absolute page coordinates by adding the box's
/// `Dimensions`, so callers don't need to track box offsets themselves.
pub fn build_glyph_list(box_: &LayoutBox) -> Vec<ClippedGlyph> {
    let mut list = Vec::new();
    collect_glyphs(box_, &mut list, None, 1.0, (0.0, 0.0));
    list
}

fn collect_glyphs(
    box_: &LayoutBox,
    out: &mut Vec<ClippedGlyph>,
    clip: Option<ClipRect>,
    parent_opacity: f64,
    parent_translate: (f64, f64),
) {
    let opacity = parent_opacity * box_.style.opacity;
    let translate = (
        parent_translate.0 + box_.style.transform.0,
        parent_translate.1 + box_.style.transform.1,
    );
    let ox = (box_.dimensions.x + translate.0) as i32;
    let oy = (box_.dimensions.y + translate.1) as i32;
    out.extend(box_.glyphs.iter().map(|g| ClippedGlyph {
        glyph: PositionedGlyph {
            x: g.x + ox,
            y: g.y + oy,
            ..*g
        },
        clip,
        opacity,
    }));
    let (child_clip, child_translate) = if box_.style.overflow == Overflow::Hidden {
        // Real per-element scroll (`ROADMAP.md` item 22) - see
        // `rects.rs`'s own `collect` for the full rationale; same shift,
        // applied to glyphs instead of background rects.
        (
            Some(tighten_clip(box_, clip, translate)),
            (
                translate.0 - box_.scroll_offset.0,
                translate.1 - box_.scroll_offset.1,
            ),
        )
    } else {
        (clip, translate)
    };
    for child in paint_order(&box_.children) {
        collect_glyphs(child, out, child_clip, opacity, child_translate);
    }
}
