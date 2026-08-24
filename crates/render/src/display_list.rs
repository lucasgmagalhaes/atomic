//! Flattens a `layout_engine::LayoutBox` tree into paint commands - a
//! "display list" in browser-engine terminology, decoupled from any GPU
//! API so it's testable without a device/adapter. Scoped to solid-color
//! rectangles (a box's border box, filled with its `background_color`,
//! plus up to 4 more solid rects framing it as a real border - see
//! `collect`'s own doc) plus glyph instances (positioned, not yet
//! rasterized) — no border-radius, no images composited here (see
//! `build_image_list`), no shadows, no clipping/scrolling.
use layout_engine::{BorderStyle, Color, LayoutBox, PositionedGlyph};

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
    if box_.style.border_style != BorderStyle::None && box_.style.border_color.a > 0 {
        push_border_rects(box_, out);
    }
    for child in &box_.children {
        collect(child, out);
    }
}

/// Real, but flat: paints `box_`'s border as up to 4 solid-color strips
/// hugging the outer edge of its (already border-inclusive, see
/// `layout_engine::layout::layout_block`'s own doc) `dimensions` - no
/// `border-radius` (a rounded corner would need real anti-aliased or
/// masked geometry this engine's "flat rects only" pipeline doesn't
/// have), no per-side colors/styles. A side with `0` resolved width
/// (`layout_engine::ResolvedBorder`) contributes no rect, same as a
/// transparent background contributing none. The 4 strips' corners
/// slightly overlap (e.g. the top strip's own left end sits under the
/// left strip's top end) rather than being mitered - harmless since
/// every side shares one solid `border_color`, so double-painting the
/// same pixel with the same color is a no-op.
fn push_border_rects(box_: &LayoutBox, out: &mut Vec<Rect>) {
    let d = box_.dimensions;
    let b = box_.border;
    let color = box_.style.border_color;
    if b.top > 0.0 {
        out.push(Rect { x: d.x as f32, y: d.y as f32, width: d.width as f32, height: b.top as f32, color });
    }
    if b.bottom > 0.0 {
        out.push(Rect { x: d.x as f32, y: (d.y + d.height - b.bottom) as f32, width: d.width as f32, height: b.bottom as f32, color });
    }
    if b.left > 0.0 {
        out.push(Rect { x: d.x as f32, y: d.y as f32, width: b.left as f32, height: d.height as f32, color });
    }
    if b.right > 0.0 {
        out.push(Rect { x: (d.x + d.width - b.right) as f32, y: d.y as f32, width: b.right as f32, height: d.height as f32, color });
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
