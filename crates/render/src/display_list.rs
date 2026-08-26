//! Flattens a `layout_engine::LayoutBox` tree into paint commands - a
//! "display list" in browser-engine terminology, decoupled from any GPU
//! API so it's testable without a device/adapter. Scoped to solid-color
//! rectangles (a box's border box, filled with its `background_color`,
//! plus up to 4 more solid rects framing it as a real border, plus one
//! more solid rect behind it all for a real (but blur-less) `box-shadow`
//! - see `collect`'s own doc) plus glyph instances (positioned, not yet
//! rasterized) — no border-radius, no images composited here (see
//! `build_image_list`).
//!
//! Real clipping now, for `overflow: hidden`/`auto`/`scroll` (see
//! `layout_engine::Overflow`'s own doc for the auto/scroll scope cut):
//! every `collect_*` walk threads an accumulated `Option<ClipRect>` -
//! `None` above the first `overflow`-clipping ancestor, then the
//! intersection of every such ancestor's own border box once one is hit
//! (a box never clips its own background/border, only its descendants'
//! paint output - matches real CSS). `Rect`s are clipped exactly (a flat
//! color fill has nothing to distort by shrinking its bounds).
//! `ImageQuad`/`PositionedGlyph` keep their full untouched geometry here
//! (an image's scale factor and a glyph's rasterized bitmap both depend
//! on their *unclipped* size/position) and instead carry the clip rect
//! forward as data for `crate::image::composite_images`/
//! `crate::text::composite_glyphs` to intersect against their own
//! per-pixel destination bounds check - real exact clipping there too,
//! not an approximation, since both already iterate pixel-by-pixel.
//!
//! Real `opacity` too, per-primitive rather than per-group (see
//! `layout_engine::ComputedStyle::opacity`'s own doc for exactly what
//! that means and diverges from the spec) - every `collect_*` walk also
//! threads an accumulated `f64` multiplier alongside `clip`, updated to
//! `parent_opacity * box_.style.opacity` at each box and applied to that
//! box's own paint primitives' alpha before they're emitted.
use layout_engine::{BorderStyle, Color, LayoutBox, Overflow, PositionedGlyph};

/// Scales `color`'s alpha channel by `opacity` (`1.0` = unchanged) -
/// shared by every paint primitive's opacity handling in this module.
fn scale_alpha(color: Color, opacity: f64) -> Color {
  Color {
    a: (color.a as f64 * opacity).round().clamp(0.0, 255.0) as u8,
    ..color
  }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
  pub color: Color,
}

/// An axis-aligned clip region in absolute page coordinates - the
/// intersection of every `overflow`-clipping ancestor's own border box
/// seen so far on a walk down the tree. Kept separate from [`Rect`]
/// (which also carries a `color`) since a clip region is pure geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipRect {
  pub x: f32,
  pub y: f32,
  pub width: f32,
  pub height: f32,
}

impl ClipRect {
  fn intersect(&self, other: &ClipRect) -> ClipRect {
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
fn clip_rect(rect: Rect, clip: Option<ClipRect>) -> Option<Rect> {
  let Some(clip) = clip else { return Some(rect) };
  let x0 = rect.x.max(clip.x);
  let y0 = rect.y.max(clip.y);
  let x1 = (rect.x + rect.width).min(clip.x + clip.width);
  let y1 = (rect.y + rect.height).min(clip.y + clip.height);
  if x1 <= x0 || y1 <= y0 {
    None
  } else {
    Some(Rect {
      x: x0,
      y: y0,
      width: x1 - x0,
      height: y1 - y0,
      color: rect.color,
    })
  }
}

/// `box_`'s own border box as a [`ClipRect`], intersected with whatever
/// clip was already in effect from an ancestor - `None` in, `None` out
/// only if `box_` itself doesn't clip either (checked by the caller
/// before calling this; this function always produces a real bound).
fn tighten_clip(box_: &LayoutBox, clip: Option<ClipRect>) -> ClipRect {
  let own = ClipRect {
    x: box_.dimensions.x as f32,
    y: box_.dimensions.y as f32,
    width: box_.dimensions.width as f32,
    height: box_.dimensions.height as f32,
  };
  match clip {
    Some(c) => c.intersect(&own),
    None => own,
  }
}

/// Walks `box_` in paint order (parent before children, so later-painted
/// children correctly draw on top of their parent's background) and
/// collects one `Rect` per box with a non-transparent background.
/// Transparent boxes are skipped rather than emitted with a zero-alpha
/// rect — nothing downstream needs to know they exist.
pub fn build_display_list(box_: &LayoutBox) -> Vec<Rect> {
  let mut list = Vec::new();
  collect(box_, &mut list, None, 1.0);
  list
}

fn collect(box_: &LayoutBox, out: &mut Vec<Rect>, clip: Option<ClipRect>, parent_opacity: f64) {
  let opacity = parent_opacity * box_.style.opacity;
  if let Some(shadow) = box_.style.box_shadow {
    if shadow.color.a > 0 {
      push_box_shadow_rect(box_, shadow, out, clip, opacity);
    }
  }
  if box_.style.background_color.a > 0 {
    let rect = Rect {
      x: box_.dimensions.x as f32,
      y: box_.dimensions.y as f32,
      width: box_.dimensions.width as f32,
      height: box_.dimensions.height as f32,
      color: scale_alpha(box_.style.background_color, opacity),
    };
    if let Some(clipped) = clip_rect(rect, clip) {
      out.push(clipped);
    }
  }
  if box_.style.border_style != BorderStyle::None && box_.style.border_color.a > 0 {
    push_border_rects(box_, out, clip, opacity);
  }
  let child_clip = if box_.style.overflow == Overflow::Hidden {
    Some(tighten_clip(box_, clip))
  } else {
    clip
  };
  for child in &box_.children {
    collect(child, out, child_clip, opacity);
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
) {
  let d = box_.dimensions;
  let rect = Rect {
    x: (d.x + shadow.offset_x - shadow.spread) as f32,
    y: (d.y + shadow.offset_y - shadow.spread) as f32,
    width: (d.width + shadow.spread * 2.0).max(0.0) as f32,
    height: (d.height + shadow.spread * 2.0).max(0.0) as f32,
    color: scale_alpha(shadow.color, opacity),
  };
  if let Some(clipped) = clip_rect(rect, clip) {
    out.push(clipped);
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
fn push_border_rects(box_: &LayoutBox, out: &mut Vec<Rect>, clip: Option<ClipRect>, opacity: f64) {
  let d = box_.dimensions;
  let b = box_.border;
  let color = scale_alpha(box_.style.border_color, opacity);
  let mut push = |rect: Rect| {
    if let Some(clipped) = clip_rect(rect, clip) {
      out.push(clipped);
    }
  };
  if b.top > 0.0 {
    push(Rect {
      x: d.x as f32,
      y: d.y as f32,
      width: d.width as f32,
      height: b.top as f32,
      color,
    });
  }
  if b.bottom > 0.0 {
    push(Rect {
      x: d.x as f32,
      y: (d.y + d.height - b.bottom) as f32,
      width: d.width as f32,
      height: b.bottom as f32,
      color,
    });
  }
  if b.left > 0.0 {
    push(Rect {
      x: d.x as f32,
      y: d.y as f32,
      width: b.left as f32,
      height: d.height as f32,
      color,
    });
  }
  if b.right > 0.0 {
    push(Rect {
      x: (d.x + d.width - b.right) as f32,
      y: d.y as f32,
      width: b.right as f32,
      height: d.height as f32,
      color,
    });
  }
}

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

/// Same paint-order walk as [`build_display_list`], but collects glyphs
/// instead of rects. `PositionedGlyph::x`/`y` are relative to the text
/// box's own top-left (see `layout_engine::text`'s docs) - this shifts
/// them to absolute page coordinates by adding the box's `Dimensions`, so
/// callers don't need to track box offsets themselves.
pub fn build_glyph_list(box_: &LayoutBox) -> Vec<ClippedGlyph> {
  let mut list = Vec::new();
  collect_glyphs(box_, &mut list, None, 1.0);
  list
}

fn collect_glyphs(
  box_: &LayoutBox,
  out: &mut Vec<ClippedGlyph>,
  clip: Option<ClipRect>,
  parent_opacity: f64,
) {
  let opacity = parent_opacity * box_.style.opacity;
  let ox = box_.dimensions.x as i32;
  let oy = box_.dimensions.y as i32;
  out.extend(box_.glyphs.iter().map(|g| ClippedGlyph {
    glyph: PositionedGlyph {
      x: g.x + ox,
      y: g.y + oy,
      ..*g
    },
    clip,
    opacity,
  }));
  let child_clip = if box_.style.overflow == Overflow::Hidden {
    Some(tighten_clip(box_, clip))
  } else {
    clip
  };
  for child in &box_.children {
    collect_glyphs(child, out, child_clip, opacity);
  }
}

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

/// Same paint-order walk as [`build_display_list`]/[`build_glyph_list`],
/// but collects one [`ImageQuad`] per box carrying a real decoded
/// `LayoutBox::image` (set by `layout_engine::apply_image_sizes`) - an
/// `<img>` with no successfully fetched/decoded image contributes
/// nothing, same as a box with a transparent background contributes no
/// [`Rect`].
pub fn build_image_list(box_: &LayoutBox) -> Vec<ImageQuad> {
  let mut list = Vec::new();
  collect_images(box_, &mut list, None, 1.0);
  list
}

fn collect_images(
  box_: &LayoutBox,
  out: &mut Vec<ImageQuad>,
  clip: Option<ClipRect>,
  parent_opacity: f64,
) {
  let opacity = parent_opacity * box_.style.opacity;
  if let Some(image) = &box_.image {
    out.push(ImageQuad {
      x: box_.dimensions.x as f32,
      y: box_.dimensions.y as f32,
      width: box_.dimensions.width as f32,
      height: box_.dimensions.height as f32,
      image: image.clone(),
      clip,
      opacity,
    });
  }
  let child_clip = if box_.style.overflow == Overflow::Hidden {
    Some(tighten_clip(box_, clip))
  } else {
    clip
  };
  for child in &box_.children {
    collect_images(child, out, child_clip, opacity);
  }
}
