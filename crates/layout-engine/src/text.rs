//! Text measurement + line-breaking via `cosmic-text`. Real shaping/
//! wrapping, not a heuristic — `cosmic_text::Buffer` handles line breaking
//! internally when given a max width, so this module is mostly plumbing
//! to get glyph positions back out in a form `render` can rasterize
//! (via `SwashCache`) without depending on `cosmic_text::Buffer` itself.
//!
//! One process-wide `FontSystem` behind a mutex, built lazily on first use
//! — loading system fonts is not cheap, and every text box sharing one
//! avoids reloading them per call. A real engine would likely scope a
//! font context per profile/tab; this crate has no such concept yet.
use std::sync::{Mutex, OnceLock};

use cosmic_text::{Attrs, Buffer, Family, FontSystem, Metrics, Shaping};

use crate::style::Color;

pub use cosmic_text::fontdb::ID as FontId;

fn font_system() -> &'static Mutex<FontSystem> {
    static FONT_SYSTEM: OnceLock<Mutex<FontSystem>> = OnceLock::new();
    FONT_SYSTEM.get_or_init(|| Mutex::new(FontSystem::new()))
}

/// One shaped glyph, positioned relative to the top-left of the text box
/// it belongs to (not the page) — `render` offsets by the box's own
/// `Dimensions` before rasterizing.
#[derive(Debug, Clone, Copy)]
pub struct PositionedGlyph {
    pub font_id: FontId,
    pub glyph_id: u16,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
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

    let mut fs = font_system().lock().unwrap();
    let line_height = font_size * 1.2; // matches CSS's `normal` line-height approximation
    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(&mut fs, metrics);
    let mut buffer = buffer.borrow_with(&mut fs);

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
            glyphs.push(PositionedGlyph {
                font_id: glyph.font_id,
                glyph_id: glyph.glyph_id,
                x: glyph.x,
                y: run.line_y + glyph.y,
                font_size: glyph.font_size,
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
