//! Block layout: walks a [`crate::tree::LayoutBox`] tree and fills in
//! [`crate::tree::Dimensions`]. Block formatting context only — children
//! stack vertically, no inline flow, no flex, no floats, no positioning
//! (`position`/`top`/`left`/... aren't modeled at all yet). `auto` margins
//! resolve to `0.0`, not CSS's actual auto-margin centering behavior.
//! No margin collapsing: adjacent margins both take full effect instead
//! of collapsing to the larger one.
//!
//! `Dimensions` stores the padding box (content + padding, no border since
//! border isn't modeled) — the box a renderer would actually paint.
use crate::style::Length;
use crate::tree::LayoutBox;

fn resolve_edge(length: Length, containing_width: f64) -> f64 {
    match length {
        Length::Px(px) => px,
        Length::Percent(pct) => containing_width * pct / 100.0,
        // Real auto-margin centering needs to know the box's own resolved
        // width first, which this helper doesn't have - callers that need
        // centering resolve width before margins. Not needed yet since
        // nothing here centers anything.
        Length::Auto => 0.0,
    }
}

/// Lays out `box_` and its subtree with `containing_width` as the
/// available width and `(x, y)` as the top-left corner of its margin box.
/// Returns the total vertical space this box (including its own margins)
/// occupies in its parent's block flow, so the caller can advance its
/// cursor by that amount for the next sibling.
pub fn layout_block(box_: &mut LayoutBox, containing_width: f64, x: f64, y: f64) -> f64 {
    let margin_top = resolve_edge(box_.style.margin.top, containing_width);
    let margin_right = resolve_edge(box_.style.margin.right, containing_width);
    let margin_bottom = resolve_edge(box_.style.margin.bottom, containing_width);
    let margin_left = resolve_edge(box_.style.margin.left, containing_width);
    let padding_top = resolve_edge(box_.style.padding.top, containing_width);
    let padding_right = resolve_edge(box_.style.padding.right, containing_width);
    let padding_bottom = resolve_edge(box_.style.padding.bottom, containing_width);
    let padding_left = resolve_edge(box_.style.padding.left, containing_width);

    // CSS's `width` property (content-box model, the only one this crate
    // models) sizes the *content* box. `auto` follows CSS 2.1 §10.3.3:
    // margin + padding + width is made to equal containing_width.
    let content_width = match box_.style.width {
        Length::Px(px) => px,
        Length::Percent(pct) => containing_width * pct / 100.0,
        Length::Auto => {
            (containing_width - margin_left - margin_right - padding_left - padding_right).max(0.0)
        }
    };

    box_.dimensions.x = x + margin_left;
    box_.dimensions.y = y + margin_top;
    box_.dimensions.width = content_width + padding_left + padding_right;

    let content_x = box_.dimensions.x + padding_left;
    let mut cursor_y = box_.dimensions.y + padding_top;

    for child in &mut box_.children {
        cursor_y += layout_block(child, content_width, content_x, cursor_y);
    }

    let content_height = cursor_y - (box_.dimensions.y + padding_top);
    let resolved_content_height = match box_.style.height {
        Length::Px(px) => px,
        // Percent height against an auto-sized container is genuinely
        // ambiguous in CSS too (resolves to auto unless the containing
        // block's height is itself definite, which this crate doesn't
        // track) - falling back to content height is a reasonable default,
        // not a spec-correct resolution.
        Length::Percent(_) => content_height,
        Length::Auto => content_height,
    };
    box_.dimensions.height = resolved_content_height + padding_top + padding_bottom;

    margin_top + box_.dimensions.height + margin_bottom
}
