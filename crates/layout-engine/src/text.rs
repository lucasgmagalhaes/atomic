//! Text measurement, line-breaking, and glyph rasterization via
//! `cosmic-text`. Real shaping/wrapping (not a heuristic — `Buffer`
//! handles line breaking internally when given a max width) and real
//! rasterization (via `SwashCache`, not a stub), returning plain alpha-
//! coverage bitmaps so `render` can composite them without depending on
//! `cosmic_text`/`swash` itself.
//!
//! One process-wide `FontSystem` + `SwashCache` behind a mutex, built
//! lazily on first use — loading system fonts isn't cheap, and every text
//! box sharing one avoids reloading them per call. A real engine would
//! likely scope a font context per profile/tab; this crate has no such
//! concept yet.
use std::sync::{Mutex, OnceLock};

use cosmic_text::{Attrs, Buffer, CacheKey, Family, FontSystem, Metrics, Shaping, SwashCache};

use crate::style::Color;

struct TextContext {
    fonts: FontSystem,
    cache: SwashCache,
}

fn context() -> &'static Mutex<TextContext> {
    static CONTEXT: OnceLock<Mutex<TextContext>> = OnceLock::new();
    CONTEXT.get_or_init(|| {
        Mutex::new(TextContext {
            fonts: FontSystem::new(),
            cache: SwashCache::new(),
        })
    })
}

/// One shaped glyph. `cache_key` plus `x`/`y` are exactly what
/// `cosmic_text::LayoutGlyph::physical()` produces — `x`/`y` are already
/// baseline-adjusted integer pixel positions relative to the text box's
/// own top-left (not the page), matching the convention cosmic-text's own
/// rendering examples use with `SwashCache::with_pixels`.
#[derive(Debug, Clone, Copy)]
pub struct PositionedGlyph {
    pub cache_key: CacheKey,
    pub x: i32,
    pub y: i32,
    pub color: Color,
}

#[derive(Debug, Clone, Default)]
pub struct TextLayout {
    pub width: f32,
    pub height: f32,
    pub glyphs: Vec<PositionedGlyph>,
}

/// Shapes and line-wraps `text` at `font_size`, constrained to `max_width`
/// if given (`None` = single unbounded line — real CSS `white-space:
/// nowrap` behavior, though that property isn't parsed/wired up yet).
/// `color` is threaded straight into every glyph since this crate has no
/// separate paint step for text yet (unlike boxes, which carry their own
/// `background_color` and let `render` read it back off the style).
pub fn layout_text(text: &str, font_size: f32, max_width: Option<f32>, color: Color) -> TextLayout {
    if text.trim().is_empty() {
        return TextLayout::default();
    }

    let mut ctx = context().lock().unwrap();
    let TextContext { fonts, .. } = &mut *ctx;

    let line_height = font_size * 1.2; // matches CSS's `normal` line-height approximation
    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(fonts, metrics);
    let mut buffer = buffer.borrow_with(fonts);

    buffer.set_size(max_width, None);
    buffer.set_text(text, &Attrs::new().family(Family::SansSerif), Shaping::Advanced, None);
    buffer.shape_until_scroll(false);

    let mut width = 0.0f32;
    let mut glyphs = Vec::new();
    let mut max_line_bottom = 0.0f32;

    for run in buffer.layout_runs() {
        width = width.max(run.line_w);
        max_line_bottom = max_line_bottom.max(run.line_top + run.line_height);
        for glyph in run.glyphs {
            let physical = glyph.physical((0.0, run.line_y), 1.0);
            glyphs.push(PositionedGlyph {
                cache_key: physical.cache_key,
                x: physical.x,
                y: physical.y,
                color,
            });
        }
    }

    TextLayout {
        width,
        height: max_line_bottom,
        glyphs,
    }
}

/// A rasterized glyph: 8-bit alpha coverage, `width * height` bytes,
/// row-major. `left`/`top` are the offset from the glyph's draw origin
/// (`PositionedGlyph::x`/`y`) to the bitmap's top-left corner — callers
/// composite at `(glyph.x + left, glyph.y - top)`, matching
/// `SwashCache::with_pixels`'s own convention (`y = -placement.top`).
pub struct GlyphBitmap {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub coverage: Vec<u8>,
}

/// Rasterizes `glyph` to an alpha-coverage bitmap, or `None` for glyphs
/// with no visible ink (e.g. spaces) or a font/cache lookup failure.
/// Color-bitmap glyphs (emoji) aren't supported — only `swash`'s `Mask`
/// content type is read, matching this engine's text-only scope.
pub fn rasterize_glyph(glyph: &PositionedGlyph) -> Option<GlyphBitmap> {
    let mut ctx = context().lock().unwrap();
    let TextContext { fonts, cache } = &mut *ctx;
    let image = cache.get_image(fonts, glyph.cache_key).as_ref()?;

    if image.content != cosmic_text::SwashContent::Mask {
        return None;
    }
    if image.placement.width == 0 || image.placement.height == 0 {
        return None;
    }

    Some(GlyphBitmap {
        left: image.placement.left,
        top: image.placement.top,
        width: image.placement.width,
        height: image.placement.height,
        coverage: image.data.clone(),
    })
}
