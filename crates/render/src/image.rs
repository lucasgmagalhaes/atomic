//! Composites decoded `<img>` pixels onto an RGBA8 pixel buffer - the
//! render-side half of the spec's "gap total" images item
//! (`mockup/rendering-engine-gaps.md` §5; decoding itself is
//! `image_decode`, intrinsic sizing is `layout_engine::apply_image_sizes`).
//! CPU-side compositing, same convention `text.rs`'s `composite_glyphs`
//! already uses for glyphs rather than a GPU texture pipeline - simpler
//! to get right, and this engine's `gpu.rs` only has a solid-color-quad
//! pipeline so far.
//!
//! Scaling: nearest-neighbor only (a box's painted size can differ from
//! its image's native pixel size - an explicit CSS `width`/`height` that
//! doesn't match, or a fractional layout size) - no bilinear/bicubic
//! filtering. Real, not a stub: an image scaled up or down samples real
//! source pixels at the nearest matching source coordinate, not a
//! flat/average color.
use crate::display_list::ImageQuad;

/// Alpha-blends (source-over) every image in `quads` onto `pixels`
/// (tightly-packed RGBA8, row-major, `width` × `height`), nearest-
/// neighbor scaled to each quad's destination rect. Pixels (or the parts
/// of a quad) outside the buffer bounds are clipped silently, same as
/// [`crate::text::composite_glyphs`].
pub fn composite_images(pixels: &mut [u8], width: u32, height: u32, quads: &[ImageQuad]) {
    for quad in quads {
        if quad.width <= 0.0 || quad.height <= 0.0 || quad.image.width == 0 || quad.image.height == 0 {
            continue;
        }
        let src = &quad.image;
        let scale_x = src.width as f32 / quad.width;
        let scale_y = src.height as f32 / quad.height;

        let dest_x0 = quad.x.floor().max(0.0) as i32;
        let dest_y0 = quad.y.floor().max(0.0) as i32;
        let dest_x1 = (quad.x + quad.width).ceil().min(width as f32) as i32;
        let dest_y1 = (quad.y + quad.height).ceil().min(height as f32) as i32;

        for py in dest_y0..dest_y1 {
            let rel_y = py as f32 - quad.y;
            if rel_y < 0.0 {
                continue;
            }
            let src_y = ((rel_y * scale_y) as u32).min(src.height - 1);
            for px in dest_x0..dest_x1 {
                let rel_x = px as f32 - quad.x;
                if rel_x < 0.0 {
                    continue;
                }
                let src_x = ((rel_x * scale_x) as u32).min(src.width - 1);

                let src_idx = ((src_y * src.width + src_x) * 4) as usize;
                let src_px = [src.rgba[src_idx], src.rgba[src_idx + 1], src.rgba[src_idx + 2], src.rgba[src_idx + 3]];
                if src_px[3] == 0 {
                    continue;
                }

                let dst_idx = ((py as u32 * width + px as u32) * 4) as usize;
                blend_over(&mut pixels[dst_idx..dst_idx + 4], src_px);
            }
        }
    }
}

/// `dst = src over dst`, both straight (non-premultiplied) RGBA8 - same
/// formula as `text.rs`'s own `blend_over` (kept as a separate copy
/// rather than sharing one function across two independent compositing
/// passes with no other reason to depend on each other).
fn blend_over(dst: &mut [u8], src: [u8; 4]) {
    let sa = src[3] as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for c in 0..3 {
        let out_c = (src[c] as f32 * sa + dst[c] as f32 * da * (1.0 - sa)) / out_a;
        dst[c] = out_c.round().clamp(0.0, 255.0) as u8;
    }
    dst[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}
