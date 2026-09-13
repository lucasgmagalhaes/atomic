//! `ImageQuad`/`build_image_list` — split out from `display_list.rs`.

use layout_engine::{LayoutBox, Overflow};

use super::helpers::{paint_order, tighten_clip, ClipRect};

/// One decoded `<img>`, positioned at its box's painted rect - the
/// destination the real pixel data should be scaled/blitted into (see
/// `crate::image::composite_images`), not necessarily the image's own
/// native pixel size (a box can be a different size than its image's
/// intrinsic dimensions, e.g. an explicit CSS `width` that doesn't match).
/// `clip` is the clip region in effect where this quad is painted (`None`
/// = unclipped) - kept as data rather than pre-shrinking `x`/`y`/`width`/
/// `height` themselves, since those also drive `composite_images`' own
/// source-pixel scale factor and shrinking them would distort the image;
/// `composite_images` intersects `clip` against its own per-pixel
/// destination bounds check instead. `opacity` is the real accumulated
/// opacity in effect where this quad is painted (`1.0` = fully opaque) -
/// `composite_images` multiplies it into each sampled pixel's alpha.
#[derive(Clone)]
pub struct ImageQuad {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub image: std::rc::Rc<image_decode::DecodedImage>,
    pub clip: Option<ClipRect>,
    pub opacity: f64,
}

/// Same paint-order walk as [`super::build_display_list`]/
/// [`super::build_glyph_list`], but collects one [`ImageQuad`] per box
/// carrying a real decoded `LayoutBox::image` (set by
/// `layout_engine::apply_image_sizes`) - an `<img>` with no successfully
/// fetched/decoded image contributes nothing, same as a box with a
/// transparent background contributes no `Rect`.
pub fn build_image_list(box_: &LayoutBox) -> Vec<ImageQuad> {
    let mut list = Vec::new();
    collect_images(box_, &mut list, None, 1.0, (0.0, 0.0));
    list
}

fn collect_images(
    box_: &LayoutBox,
    out: &mut Vec<ImageQuad>,
    clip: Option<ClipRect>,
    parent_opacity: f64,
    parent_translate: (f64, f64),
) {
    let opacity = parent_opacity * box_.style.opacity;
    let translate = (
        parent_translate.0 + box_.style.transform.0,
        parent_translate.1 + box_.style.transform.1,
    );
    if let Some(image) = &box_.image {
        out.push(ImageQuad {
            x: (box_.dimensions.x + translate.0) as f32,
            y: (box_.dimensions.y + translate.1) as f32,
            width: box_.dimensions.width as f32,
            height: box_.dimensions.height as f32,
            image: image.clone(),
            clip,
            opacity,
        });
    }
    let (child_clip, child_translate) = if box_.style.overflow == Overflow::Hidden {
        // Real per-element scroll (`ROADMAP.md` item 22) - see
        // `rects.rs`'s own `collect` for the full rationale; same shift,
        // applied to images instead of background rects.
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
        collect_images(child, out, child_clip, opacity, child_translate);
    }
}
