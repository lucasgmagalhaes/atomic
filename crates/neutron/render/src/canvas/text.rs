//! `ctx.font`/`fillText`/`strokeText`/`measureText` - real shaping/
//! rasterization via `layout_engine::layout_text` and
//! `render::text::composite_glyphs`, the same pipeline page text already
//! uses.
use layout_engine::{Color, FontFamily, GenericFontFamily};

use crate::display_list::ClippedGlyph;

use super::Canvas2D;

/// `ctx.font`'s default size (real spec's own default is `10px` - larger
/// here for legibility, not a spec match) until [`Canvas2D::set_font`]
/// changes it.
pub(super) const DEFAULT_FONT_SIZE: f32 = 16.0;

impl Canvas2D {
    /// `ctx.font = "<size>px <family>"` — scoped to exactly that shape:
    /// a pixel size (`px` unit required) followed by one generic family
    /// keyword (`sans-serif`/`serif`/`monospace`/`cursive`/`fantasy`) or a
    /// named-plus-generic pair (`"Arial, sans-serif"`, matching
    /// `layout_engine::FontFamily::named`'s own one-name-plus-fallback
    /// shape). No weight/style/line-height/multi-family-list (real spec's
    /// full font shorthand) - an unparseable string is silently ignored,
    /// leaving the previous font in place, same "best-effort, no separate
    /// error path" convention this crate's other setters already take.
    pub fn set_font(&mut self, css_font: &str) {
        let mut parts = css_font.trim().splitn(2, char::is_whitespace);
        let Some(size_token) = parts.next() else {
            return;
        };
        let Some(px_str) = size_token.strip_suffix("px") else {
            return;
        };
        let Ok(size) = px_str.parse::<f32>() else {
            return;
        };
        if size <= 0.0 {
            return;
        }
        let family_str = parts.next().unwrap_or("sans-serif").trim();
        let (name, generic_str) = match family_str.split_once(',') {
            Some((name, generic)) => (Some(name.trim()), generic.trim()),
            None => (None, family_str),
        };
        let generic = match generic_str {
            "serif" => GenericFontFamily::Serif,
            "monospace" => GenericFontFamily::Monospace,
            "cursive" => GenericFontFamily::Cursive,
            "fantasy" => GenericFontFamily::Fantasy,
            _ => GenericFontFamily::SansSerif,
        };
        self.font_size = size;
        self.font_family = match name {
            Some(name) if !name.is_empty() => FontFamily::named(name, generic),
            _ => FontFamily::generic(generic),
        };
    }

    /// `ctx.font`'s getter side - formats the current size/family back
    /// into the same `"<size>px <family>"` shape [`Self::set_font`]
    /// parses, not necessarily byte-identical to whatever string a caller
    /// originally set (e.g. `"16.0px sans-serif"` normalizes to
    /// `"16px sans-serif"`), same "canonical re-format, not verbatim
    /// echo" convention `fill_style()`'s own hex getter already takes.
    pub fn font(&self) -> String {
        let generic = match self.font_family.generic {
            GenericFontFamily::Serif => "serif",
            GenericFontFamily::SansSerif => "sans-serif",
            GenericFontFamily::Monospace => "monospace",
            GenericFontFamily::Cursive => "cursive",
            GenericFontFamily::Fantasy => "fantasy",
        };
        match self.font_family.name() {
            Some(name) => format!("{}px {}, {}", self.font_size, name, generic),
            None => format!("{}px {}", self.font_size, generic),
        }
    }

    /// `ctx.measureText(text).width` — real shaped width via the same
    /// `layout_engine::layout_text` `fillText` itself uses, at the
    /// current `font`. Only `width` (a real `TextMetrics` has a dozen
    /// more fields - ascent/descent/bounding-box variants - none of which
    /// this crate tracks).
    pub fn measure_text(&self, text: &str) -> f32 {
        if text.is_empty() {
            return 0.0;
        }
        layout_engine::layout_text(
            text,
            self.font_size,
            None,
            self.fill_style,
            self.font_family,
        )
        .width
    }

    /// Shared by [`Self::fill_text`]/[`Self::stroke_text`] - shapes `text`
    /// at the current `font` and composites it in `color`, via the same
    /// `get_image_data`/`composite_glyphs`/`put_image_data` round trip
    /// either way (see [`Self::fill_text`]'s own doc for why).
    fn draw_text(&mut self, text: &str, x: f32, y: f32, color: Color) {
        if text.is_empty() {
            return;
        }
        let layout =
            layout_engine::layout_text(text, self.font_size, None, color, self.font_family);
        if layout.glyphs.is_empty() {
            return;
        }
        let (tx, ty) = (
            (x + self.translate_x).round() as i32,
            (y + self.translate_y).round() as i32,
        );
        let glyphs: Vec<ClippedGlyph> = layout
            .glyphs
            .iter()
            .map(|g| {
                let mut positioned = *g;
                positioned.x += tx;
                positioned.y += ty;
                ClippedGlyph {
                    glyph: positioned,
                    clip: None,
                    opacity: 1.0,
                    fixed: false,
                    sticky: None,
                }
            })
            .collect();

        let mut pixels = self.get_image_data();
        crate::text::composite_glyphs(&mut pixels, self.width, self.height, &glyphs);
        self.put_image_data(0, 0, self.width, self.height, &pixels);
    }

    /// `ctx.fillText(text, x, y)` — real shaping/rasterization via
    /// `layout_engine::layout_text` (`cosmic-text`+`swash`, the same real
    /// pipeline page text uses) and `render::text::composite_glyphs` (the
    /// same CPU alpha-blend compositor page text uses), not a stub. Uses
    /// the current `font`/`fillStyle` (see [`Self::set_font`]).
    ///
    /// Scope cuts: no text wrapping (`x`, `y` only - the `maxWidth` 4th
    /// argument isn't accepted, matching `layout_text`'s own
    /// `max_width: None` = single unbounded line). `y` behaves like real
    /// spec's `textBaseline = "top"` (measured from the text's own top,
    /// not the default `"alphabetic"` baseline) - this reuses
    /// `layout_text`'s glyphs exactly as `layout_engine`'s own page-text
    /// pipeline positions them (box-top-relative), with no separate
    /// baseline-offset math added on top. No gradient fill (`fillStyle`'s
    /// plain color only).
    pub fn fill_text(&mut self, text: &str, x: f32, y: f32) {
        let color = self.fill_style;
        self.draw_text(text, x, y, color);
    }

    /// `ctx.strokeText(text, x, y)` — **not a real outline stroke**: this
    /// crate has no glyph-outline/path data to stroke, only alpha-coverage
    /// bitmaps from `layout_engine::rasterize_glyph`, which can be filled
    /// but not traced as a path. Scoped instead to painting the same
    /// glyphs solid in `strokeStyle`'s color - visually close for normal
    /// text sizes (a filled glyph vs. a thin-outlined one), but genuinely
    /// different from spec at any lineWidth/size where the distinction
    /// would be visible. Otherwise identical scope cuts to
    /// [`Self::fill_text`].
    pub fn stroke_text(&mut self, text: &str, x: f32, y: f32) {
        let color = self.stroke_style;
        self.draw_text(text, x, y, color);
    }
}
