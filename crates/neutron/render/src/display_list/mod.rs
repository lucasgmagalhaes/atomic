//! Flattens a `layout_engine::LayoutBox` tree into paint commands - a
//! "display list" in browser-engine terminology, decoupled from any GPU
//! API so it's testable without a device/adapter. Scoped to solid-color
//! rectangles (a box's border box, filled with its `background_color`,
//! plus up to 4 more solid rects framing it as a real border, plus one
//! more solid rect behind it all for a real (but blur-less) `box-shadow`
//! - see `rects::collect`'s own doc) plus glyph instances (positioned, not
//! yet rasterized) — no border-radius, no images composited here (see
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
//!
//! Split into `helpers.rs` (paint-order sorting, alpha scaling, and clip-
//! region helpers, plus `ClipRect` itself), `rects.rs` (`Rect`/
//! `build_display_list`), `glyphs.rs` (`ClippedGlyph`/`build_glyph_list`),
//! and `images.rs` (`ImageQuad`/`build_image_list`).

mod glyphs;
mod helpers;
mod images;
mod rects;

pub use glyphs::{build_glyph_list, ClippedGlyph};
pub use helpers::ClipRect;
pub use images::{build_image_list, ImageQuad};
pub use rects::{build_display_list, Rect};
