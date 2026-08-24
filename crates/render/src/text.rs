//! Composites shaped glyphs onto an RGBA8 pixel buffer via
//! `layout_engine::rasterize_glyph` (real `swash`-backed rasterization,
//! not a stub). CPU-side compositing, not GPU-accelerated — unlike
//! `gpu`/`canvas`'s wgpu pipelines, this just writes bytes directly into
//! the readback buffer. Simpler to get right than a glyph-atlas texture
//! pipeline, and correct rendering matters more than throughput at this
//! stage; revisit if/when text becomes a performance bottleneck.
use crate::display_list::{ClipRect, ClippedGlyph};

/// Alpha-blends (source-over) every glyph in `glyphs` onto `pixels`
/// (tightly-packed RGBA8, row-major, `width` × `height`). Glyphs (or the
/// parts of them) outside the buffer bounds are clipped silently, same as
/// a real canvas would clip off-screen drawing - and, now, also clipped
/// against each glyph's own `ClippedGlyph::clip` (real `overflow: hidden`
/// content clipping, see `display_list`'s own doc), exactly, since this
/// already checks per-pixel bounds. Also multiplies each pixel's alpha by
/// `ClippedGlyph::opacity` (real `opacity`, see `display_list`'s own doc
/// on its per-primitive scope).
pub fn composite_glyphs(pixels: &mut [u8], width: u32, height: u32, glyphs: &[ClippedGlyph]) {
    for ClippedGlyph { glyph, clip, opacity } in glyphs {
        if *opacity <= 0.0 {
            continue;
        }
        let Some(bitmap) = layout_engine::rasterize_glyph(glyph) else {
            continue;
        };

        // Matches SwashCache::with_pixels' own convention: the bitmap's
        // top-left is at (glyph.x + placement.left, glyph.y - placement.top).
        let origin_x = glyph.x + bitmap.left;
        let origin_y = glyph.y - bitmap.top;

        for row in 0..bitmap.height as i32 {
            let py = origin_y + row;
            if py < 0 || py >= height as i32 {
                continue;
            }
            for col in 0..bitmap.width as i32 {
                let px = origin_x + col;
                if px < 0 || px >= width as i32 {
                    continue;
                }
                if !in_clip(px, py, clip) {
                    continue;
                }

                let coverage = bitmap.coverage[(row as u32 * bitmap.width + col as u32) as usize];
                if coverage == 0 {
                    continue;
                }
                let alpha = (coverage as f64 * glyph.color.a as f64 * opacity / 255.0).round().clamp(0.0, 255.0) as u8;
                if alpha == 0 {
                    continue;
                }

                let idx = ((py as u32 * width + px as u32) * 4) as usize;
                let src = [glyph.color.r, glyph.color.g, glyph.color.b, alpha];
                blend_over(&mut pixels[idx..idx + 4], src);
            }
        }
    }
}

/// Whether pixel `(px, py)` (integer buffer coordinates) falls inside
/// `clip` - `None` means unclipped, always `true`. Real `overflow`
/// clipping check, shared by every pixel this function writes.
fn in_clip(px: i32, py: i32, clip: &Option<ClipRect>) -> bool {
    let Some(clip) = clip else { return true };
    let x = px as f32;
    let y = py as f32;
    x >= clip.x && x < clip.x + clip.width && y >= clip.y && y < clip.y + clip.height
}

/// `dst = src over dst`, both straight (non-premultiplied) RGBA8.
fn blend_over(dst: &mut [u8], src: [u8; 4]) {
    let sa = src[3] as f32 / 255.0;
    let da = dst[3] as f32 / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }
    for c in 0..3 {
        let s = src[c] as f32 / 255.0;
        let d = dst[c] as f32 / 255.0;
        let out = (s * sa + d * da * (1.0 - sa)) / out_a;
        dst[c] = (out * 255.0).round() as u8;
    }
    dst[3] = (out_a * 255.0).round() as u8;
}
