//! `ctx.createPattern(image, repetition)` — a CPU-side tiled fill, built
//! entirely from the existing `get_image_data`/`put_image_data` round
//! trip (same convention `drawImage`'s own canvas-to-canvas copy and
//! `fillText`'s glyph compositing already use), no new GPU pipeline.
//!
//! Scoped to `repetition: 'repeat'` only (real spec's own default, and by
//! far the most common case) — `'repeat-x'`/`'repeat-y'`/`'no-repeat'`
//! are accepted but silently fall back to full `'repeat'` tiling. This
//! isn't an arbitrary cut: a full `'repeat'` tile always covers the
//! entire destination rect with a real source pixel, so a plain
//! hard-overwrite `put_image_data` call is exactly correct; the partial-
//! coverage modes would leave some destination pixels untouched by the
//! pattern (real spec: the pre-existing canvas content shows through
//! there), which `put_image_data`'s hard-replace semantics can't express
//! without a second, separate blended-write path this pass doesn't add.
//!
//! Like gradient fills, a pattern-filled rect's own corners *are* run
//! through the current transform matrix (same `transformed_corners`
//! path solid/gradient rects use for *where* they land), but the tiling
//! itself is computed in axis-aligned destination-pixel space, ignoring
//! any `scale`/`rotate` component of the transform — a rotated
//! pattern-filled rect still tiles axis-aligned pixels, not rotated
//! ones, matching gradient fills' own "not run through the transform"
//! cut in spirit (this crate has no per-pixel inverse-transform sampling
//! anywhere).
use super::Canvas2D;

#[derive(Clone)]
pub struct Pattern {
    pub source_width: u32,
    pub source_height: u32,
    /// Tightly packed RGBA8, `source_width * source_height * 4` bytes -
    /// same shape [`Canvas2D::get_image_data`]/[`Canvas2D::put_image_data`]
    /// already use.
    pub source_pixels: Vec<u8>,
}

impl Canvas2D {
    /// `ctx.fillStyle = pattern` — like [`Canvas2D::set_fill_gradient`],
    /// overrides `fill_style`/`fill_gradient` for `fill_rect` only,
    /// without touching the plain solid-color fallback underneath.
    pub fn set_fill_pattern(&mut self, pattern: Pattern) {
        self.fill_gradient = None;
        self.fill_pattern = Some(pattern);
    }

    /// `Some` when the current fill paint is a pattern.
    pub fn fill_pattern(&self) -> Option<&Pattern> {
        self.fill_pattern.as_ref()
    }

    /// Same as `shapes`'s private `draw_rect`/`gradients::draw_gradient_rect`
    /// but tiling `pattern`'s source pixels instead of a solid color or a
    /// GPU-interpolated gradient. See this module's own doc for the
    /// `'repeat'`-only scope cut and why the current transform's
    /// `scale`/`rotate` component doesn't affect the tiling itself.
    /// `pub(super)` since `shapes::fill_rect` dispatches to this.
    pub(super) fn draw_pattern_rect(&mut self, x: f32, y: f32, w: f32, h: f32, pattern: &Pattern) {
        let (cw, ch) = (self.width as f32, self.height as f32);
        let x0 = x.max(0.0).round() as i32;
        let y0 = y.max(0.0).round() as i32;
        let x1 = (x + w).min(cw).round() as i32;
        let y1 = (y + h).min(ch).round() as i32;
        if x1 <= x0 || y1 <= y0 || pattern.source_width == 0 || pattern.source_height == 0 {
            return;
        }
        let (rw, rh) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let mut buf = vec![0u8; (rw * rh * 4) as usize];
        for ry in 0..rh {
            let sy = ((y0 + ry as i32).rem_euclid(pattern.source_height as i32)) as u32;
            for rx in 0..rw {
                let sx = ((x0 + rx as i32).rem_euclid(pattern.source_width as i32)) as u32;
                let src = ((sy * pattern.source_width + sx) * 4) as usize;
                let dst = ((ry * rw + rx) * 4) as usize;
                buf[dst..dst + 4].copy_from_slice(&pattern.source_pixels[src..src + 4]);
            }
        }
        self.put_image_data(x0, y0, rw, rh, &buf);
    }
}
