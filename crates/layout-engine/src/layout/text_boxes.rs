//! `layout_text_box`/`layout_inline_box` — split out from `layout.rs`.

use crate::text::{layout_inline, layout_text, InlineSpan};
use crate::tree::LayoutBox;

/// Lays out a text leaf box: measures/wraps it against `containing_width`
/// via `cosmic-text` and sets its dimensions/glyphs directly, bypassing
/// the normal box-model resolution entirely (a text box has no margin/
/// padding/explicit width/height to resolve from style - its size comes
/// purely from its shaped content). Returns the vertical space consumed,
/// same convention as `layout_block`.
pub(super) fn layout_text_box(box_: &mut LayoutBox, containing_width: f64, x: f64, y: f64) -> f64 {
    let text = box_.text.as_deref().unwrap_or("");
    let result = layout_text(
        text,
        box_.style.font_size as f32,
        Some(containing_width as f32),
        box_.style.color,
    );

    box_.dimensions.x = x;
    box_.dimensions.y = y;
    box_.dimensions.width = result.width as f64;
    box_.dimensions.height = result.height as f64;
    box_.glyphs = result.glyphs;

    box_.dimensions.height
}

/// Lays out a synthetic inline-formatting-context box (see
/// `tree::LayoutBox::inline_spans`): shapes every span in the run together
/// as one paragraph via `text::layout_inline`, same bypass-the-box-model
/// approach as `layout_text_box` since an inline run has no box-model
/// properties of its own either. Returns the vertical space consumed.
pub(super) fn layout_inline_box(
    box_: &mut LayoutBox,
    containing_width: f64,
    x: f64,
    y: f64,
) -> f64 {
    let sources = box_.inline_spans.as_deref().unwrap_or(&[]);
    let spans: Vec<InlineSpan> = sources
        .iter()
        .map(|s| InlineSpan {
            text: &s.text,
            font_size: s.font_size as f32,
            color: s.color,
        })
        .collect();
    let result = layout_inline(&spans, Some(containing_width as f32));

    box_.dimensions.x = x;
    box_.dimensions.y = y;
    box_.dimensions.width = result.width as f64;
    box_.dimensions.height = result.height as f64;
    box_.glyphs = result.glyphs;

    box_.dimensions.height
}
