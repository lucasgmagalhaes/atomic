//! `layout_block` — split out from `layout.rs`.

use crate::style::{BorderStyle, Length, Position};
use crate::tree::LayoutBox;

use super::children::layout_children;
use super::offsets::{offset_from_edges, resolve_edge};
use super::text_boxes::{layout_inline_box, layout_text_box};

/// Lays out `box_` and its subtree with `containing_width` as the
/// available width and `(x, y)` as the top-left corner of its margin box.
/// Returns the total vertical space this box (including its own margins)
/// occupies in its parent's block flow, so the caller can advance its
/// cursor by that amount for the next sibling. Used for block children;
/// flex items are positioned by `flex::layout_flex_children` instead,
/// which calls `layout_children` directly once the item's own outer box
/// is already fixed by the flex algorithm.
pub fn layout_block(box_: &mut LayoutBox, containing_width: f64, x: f64, y: f64) -> f64 {
    if box_.text.is_some() {
        return layout_text_box(box_, containing_width, x, y);
    }
    if box_.inline_spans.is_some() {
        return layout_inline_box(box_, containing_width, x, y);
    }

    // Real `position: relative`: a purely visual shift applied to where
    // this box (and, since everything below derives `content_x`/
    // `content_y` from `box_.dimensions.x`/`y`, its whole subtree) actually
    // paints — the space it occupies in its parent's flow is untouched,
    // since the return value below is computed from `margin`/`height`
    // alone, never from `x`/`y`. Matches the real spec's own "as if
    // position were static, then offset visually" behavior.
    let (x, y) = if box_.style.position == Position::Relative {
        let (dx, dy) = offset_from_edges(&box_.style);
        (x + dx, y + dy)
    } else {
        (x, y)
    };

    let margin_top = resolve_edge(box_.style.margin.top, containing_width);
    let margin_right = resolve_edge(box_.style.margin.right, containing_width);
    let margin_bottom = resolve_edge(box_.style.margin.bottom, containing_width);
    let margin_left = resolve_edge(box_.style.margin.left, containing_width);
    let padding_top = resolve_edge(box_.style.padding.top, containing_width);
    let padding_right = resolve_edge(box_.style.padding.right, containing_width);
    let padding_bottom = resolve_edge(box_.style.padding.bottom, containing_width);
    let padding_left = resolve_edge(box_.style.padding.left, containing_width);
    // `border_width` is `0` on every side whose `border-style` is `None`
    // (the real spec's own rule: a border with no style renders as if its
    // width were `0`, regardless of what `border-width` itself says) -
    // real box-model growth, not just a paint-time decoration, so a
    // bordered box genuinely takes more space than an unbordered one with
    // otherwise-identical CSS (see `render::build_display_list` for the
    // actual stroke painting).
    let (border_top, border_right, border_bottom, border_left) =
        if box_.style.border_style == BorderStyle::None {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (
                resolve_edge(box_.style.border_width.top, containing_width),
                resolve_edge(box_.style.border_width.right, containing_width),
                resolve_edge(box_.style.border_width.bottom, containing_width),
                resolve_edge(box_.style.border_width.left, containing_width),
            )
        };
    box_.border = crate::tree::ResolvedBorder {
        top: border_top,
        right: border_right,
        bottom: border_bottom,
        left: border_left,
    };

    // CSS's `width` property (content-box model, the only one this crate
    // models) sizes the *content* box. `auto` follows CSS 2.1 §10.3.3:
    // margin + border + padding + width is made to equal containing_width.
    let content_width = match box_.style.width {
        Length::Px(px) => px,
        Length::Percent(pct) => containing_width * pct / 100.0,
        Length::Auto => (containing_width
            - margin_left
            - margin_right
            - border_left
            - border_right
            - padding_left
            - padding_right)
            .max(0.0),
    };

    box_.dimensions.x = x + margin_left;
    box_.dimensions.y = y + margin_top;
    box_.dimensions.width =
        content_width + padding_left + padding_right + border_left + border_right;

    let content_x = box_.dimensions.x + border_left + padding_left;
    let content_y = box_.dimensions.y + border_top + padding_top;

    let content_height = layout_children(box_, content_width, content_x, content_y);

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
    box_.dimensions.height =
        resolved_content_height + padding_top + padding_bottom + border_top + border_bottom;

    margin_top + box_.dimensions.height + margin_bottom
}
