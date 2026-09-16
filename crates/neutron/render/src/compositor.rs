//! Whole-canvas alpha compositing (`ROADMAP.md` item 28) — blends one
//! already-rendered layer canvas onto another, both tightly-packed RGBA8,
//! row-major, same `width`/`height`. Unlike [`crate::image::composite_images`]/
//! [`crate::text::composite_glyphs`] (which blend individual primitives as
//! they walk a display list), this blends two whole pre-rendered pixel
//! buffers — the base frame and one compositing layer's own transparent-
//! background canvas (see `layer.rs`'s own doc for why a layer needs an
//! independent cache in the first place).
//!
//! Same `src-over` formula as `image.rs`/`text.rs`'s own `blend_over` —
//! copied rather than shared, matching this crate's existing convention of
//! one small, local compositing helper per file instead of one shared
//! utility.

/// Alpha-blends every pixel of `layer` onto `base` (source-over), both
/// tightly-packed RGBA8 buffers of the same `width` × `height`. A fully
/// transparent `layer` pixel leaves `base` untouched; a fully opaque one
/// replaces it outright.
pub fn composite_layer_onto(base: &mut [u8], layer: &[u8], width: u32, height: u32) {
    let pixel_count = (width as usize) * (height as usize);
    debug_assert_eq!(base.len(), pixel_count * 4);
    debug_assert_eq!(layer.len(), pixel_count * 4);

    for i in 0..pixel_count {
        let idx = i * 4;
        let src = [layer[idx], layer[idx + 1], layer[idx + 2], layer[idx + 3]];
        if src[3] == 0 {
            continue;
        }
        blend_over(&mut base[idx..idx + 4], src);
    }
}

/// `dst = src over dst`, both straight (non-premultiplied) RGBA8 — same
/// formula as `image.rs`/`text.rs`'s own `blend_over`.
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
