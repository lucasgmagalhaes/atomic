//! Flattens a `layout_engine::LayoutBox` tree into paint commands - a
//! "display list" in browser-engine terminology, decoupled from any GPU
//! API so it's testable without a device/adapter. Scoped to solid-color
//! rectangles (a box's padding box, filled with its `background_color`)
//! plus glyph instances (positioned, not yet rasterized) — no borders,
//! no images, no shadows, no clipping/scrolling.
use layout_engine::{Color, LayoutBox, PositionedGlyph};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Color,
}

/// Walks `box_` in paint order (parent before children, so later-painted
/// children correctly draw on top of their parent's background) and
/// collects one `Rect` per box with a non-transparent background.
/// Transparent boxes are skipped rather than emitted with a zero-alpha
/// rect — nothing downstream needs to know they exist.
pub fn build_display_list(box_: &LayoutBox) -> Vec<Rect> {
    let mut list = Vec::new();
    collect(box_, &mut list);
    list
}

fn collect(box_: &LayoutBox, out: &mut Vec<Rect>) {
    if box_.style.background_color.a > 0 {
        out.push(Rect {
            x: box_.dimensions.x as f32,
            y: box_.dimensions.y as f32,
            width: box_.dimensions.width as f32,
            height: box_.dimensions.height as f32,
            color: box_.style.background_color,
        });
    }
    for child in &box_.children {
        collect(child, out);
    }
}

/// Same paint-order walk as [`build_display_list`], but collects glyphs
/// instead of rects. `PositionedGlyph::x`/`y` are relative to the text
/// box's own top-left (see `layout_engine::text`'s docs) - this shifts
/// them to absolute page coordinates by adding the box's `Dimensions`, so
/// callers don't need to track box offsets themselves.
pub fn build_glyph_list(box_: &LayoutBox) -> Vec<PositionedGlyph> {
    let mut list = Vec::new();
    collect_glyphs(box_, &mut list);
    list
}

fn collect_glyphs(box_: &LayoutBox, out: &mut Vec<PositionedGlyph>) {
    let ox = box_.dimensions.x as i32;
    let oy = box_.dimensions.y as i32;
    out.extend(box_.glyphs.iter().map(|g| PositionedGlyph {
        x: g.x + ox,
        y: g.y + oy,
        ..*g
    }));
    for child in &box_.children {
        collect_glyphs(child, out);
    }
}

/// One decoded `<img>`, positioned at its box's painted rect - the
/// destination the real pixel data should be scaled/blitted into (see
/// `crate::image::composite_images`), not necessarily the image's own
/// native pixel size (a box can be a different size than its image's
/// intrinsic dimensions, e.g. an explicit CSS `width` that doesn't match).
#[derive(Clone)]
pub struct ImageQuad {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub image: std::rc::Rc<image_decode::DecodedImage>,
}

/// Same paint-order walk as [`build_display_list`]/[`build_glyph_list`],
/// but collects one [`ImageQuad`] per box carrying a real decoded
/// `LayoutBox::image` (set by `layout_engine::apply_image_sizes`) - an
/// `<img>` with no successfully fetched/decoded image contributes
/// nothing, same as a box with a transparent background contributes no
/// [`Rect`].
pub fn build_image_list(box_: &LayoutBox) -> Vec<ImageQuad> {
    let mut list = Vec::new();
    collect_images(box_, &mut list);
    list
}

fn collect_images(box_: &LayoutBox, out: &mut Vec<ImageQuad>) {
    if let Some(image) = &box_.image {
        out.push(ImageQuad {
            x: box_.dimensions.x as f32,
            y: box_.dimensions.y as f32,
            width: box_.dimensions.width as f32,
            height: box_.dimensions.height as f32,
            image: image.clone(),
        });
    }
    for child in &box_.children {
        collect_images(child, out);
    }
}
