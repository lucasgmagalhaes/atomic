//! Flex layout: single line only (no wrapping), row/column direction.
//! Simplifications versus the real spec:
//! - `flex-basis: auto` falls back to the matching width/height property,
//!   and to `0` if that's also auto — there's no content-based ("shrink to
//!   fit") sizing anywhere in this engine yet, so `0` is the only sane
//!   default rather than measuring content.
//! - `align-items` only stretches/positions a child when the container's
//!   cross size is known (explicit `height` for row, or always for
//!   column, since cross=width is resolved before children in both
//!   directions) — when the row container's height is auto, cross size
//!   comes from each child's own height property (`0` if that's auto too).
//! - No `flex-wrap`, no `order`, no `align-self`, no `gap`.
use crate::layout::{layout_children, resolve_edge};
use crate::style::{AlignItems, FlexDirection, JustifyContent, Length};
use crate::tree::LayoutBox;

fn resolve_opt(length: Length, containing: Option<f64>) -> Option<f64> {
    match length {
        Length::Px(px) => Some(px),
        Length::Percent(pct) => containing.map(|c| c * pct / 100.0),
        Length::Auto => None,
    }
}

fn basis_of(child: &LayoutBox, is_row: bool, main_known: Option<f64>) -> f64 {
    if let Some(v) = resolve_opt(child.style.flex_basis, main_known) {
        return v;
    }
    let main_prop = if is_row {
        child.style.width
    } else {
        child.style.height
    };
    resolve_opt(main_prop, main_known).unwrap_or(0.0)
}

fn cross_of(child: &LayoutBox, is_row: bool, cross_known: Option<f64>) -> Option<f64> {
    let cross_prop = if is_row {
        child.style.height
    } else {
        child.style.width
    };
    resolve_opt(cross_prop, cross_known)
}

pub struct FlexResult {
    pub main_size: f64,
    pub cross_size: f64,
}

/// Lays out `container`'s direct children as flex items and recurses into
/// each child's own children. `main_known`/`cross_known` are the
/// container's own resolved main/cross content sizes, if definite (always
/// `Some` for the cross axis when `is_row` is false, since width is
/// resolved before any children pass regardless of direction).
pub fn layout_flex_children(
    container: &mut LayoutBox,
    main_known: Option<f64>,
    cross_known: Option<f64>,
    content_x: f64,
    content_y: f64,
) -> FlexResult {
    let is_row = container.style.flex_direction == FlexDirection::Row;
    let justify = container.style.justify_content;
    let align = container.style.align_items;
    let n = container.children.len();

    let basis: Vec<f64> = container
        .children
        .iter()
        .map(|c| basis_of(c, is_row, main_known))
        .collect();
    let total_basis: f64 = basis.iter().sum();
    let total_grow: f64 = container.children.iter().map(|c| c.style.flex_grow).sum();
    let total_shrink_weighted: f64 = container
        .children
        .iter()
        .zip(&basis)
        .map(|(c, b)| c.style.flex_shrink * b)
        .sum();

    let resolved_main = main_known.unwrap_or(total_basis);
    let mut main_sizes = basis.clone();
    let mut leftover = 0.0;

    if let Some(main) = main_known {
        let free = main - total_basis;
        if free > 0.0 && total_grow > 0.0 {
            for (i, c) in container.children.iter().enumerate() {
                main_sizes[i] = basis[i] + free * (c.style.flex_grow / total_grow);
            }
        } else if free < 0.0 && total_shrink_weighted > 0.0 {
            for (i, c) in container.children.iter().enumerate() {
                let weight = c.style.flex_shrink * basis[i];
                let delta = free * (weight / total_shrink_weighted);
                main_sizes[i] = (basis[i] + delta).max(0.0);
            }
        } else {
            leftover = free;
        }
    }

    let cross_sizes: Vec<f64> = container
        .children
        .iter()
        .map(|c| match (align, cross_known) {
            (AlignItems::Stretch, Some(cross)) => cross,
            _ => cross_of(c, is_row, cross_known).unwrap_or(0.0),
        })
        .collect();
    let resolved_cross =
        cross_known.unwrap_or_else(|| cross_sizes.iter().cloned().fold(0.0, f64::max));

    let gap_count = n.saturating_sub(1);
    let (start_offset, gap) = match justify {
        JustifyContent::Start => (0.0, 0.0),
        JustifyContent::End => (leftover, 0.0),
        JustifyContent::Center => (leftover / 2.0, 0.0),
        JustifyContent::SpaceBetween => (
            0.0,
            if gap_count > 0 {
                leftover / gap_count as f64
            } else {
                0.0
            },
        ),
        JustifyContent::SpaceAround => {
            let g = if n > 0 { leftover / n as f64 } else { 0.0 };
            (g / 2.0, g)
        }
    };

    // Percentages on margin/padding always resolve against the containing
    // block's *width*, never its height - true for every CSS box, not
    // just flex items. That's `main_known` for row (main axis = width) or
    // `cross_known` for column (cross axis = width).
    let percent_base = if is_row { main_known } else { cross_known }.unwrap_or(0.0);

    let mut main_cursor = start_offset;
    for (i, child) in container.children.iter_mut().enumerate() {
        let m_top = resolve_edge(child.style.margin.top, percent_base);
        let m_right = resolve_edge(child.style.margin.right, percent_base);
        let m_bottom = resolve_edge(child.style.margin.bottom, percent_base);
        let m_left = resolve_edge(child.style.margin.left, percent_base);
        let p_top = resolve_edge(child.style.padding.top, percent_base);
        let p_right = resolve_edge(child.style.padding.right, percent_base);
        let p_bottom = resolve_edge(child.style.padding.bottom, percent_base);
        let p_left = resolve_edge(child.style.padding.left, percent_base);

        let (m_main_start, m_main_end, m_cross_start, m_cross_end) = if is_row {
            (m_left, m_right, m_top, m_bottom)
        } else {
            (m_top, m_bottom, m_left, m_right)
        };
        let (p_main_start, p_main_end, p_cross_start, p_cross_end) = if is_row {
            (p_left, p_right, p_top, p_bottom)
        } else {
            (p_top, p_bottom, p_left, p_right)
        };

        let content_main = main_sizes[i];
        let content_cross = cross_sizes[i];
        let padding_box_main = content_main + p_main_start + p_main_end;
        let padding_box_cross = content_cross + p_cross_start + p_cross_end;

        let cross_offset = match align {
            AlignItems::Start | AlignItems::Stretch => 0.0,
            AlignItems::End => resolved_cross - padding_box_cross - m_cross_start - m_cross_end,
            AlignItems::Center => {
                (resolved_cross - padding_box_cross - m_cross_start - m_cross_end) / 2.0
            }
        };

        let main_pos = main_cursor + m_main_start;
        let cross_pos = cross_offset + m_cross_start;

        let (dx, dy, dw, dh) = if is_row {
            (
                content_x + main_pos,
                content_y + cross_pos,
                padding_box_main,
                padding_box_cross,
            )
        } else {
            (
                content_x + cross_pos,
                content_y + main_pos,
                padding_box_cross,
                padding_box_main,
            )
        };
        child.dimensions.x = dx;
        child.dimensions.y = dy;
        child.dimensions.width = dw;
        child.dimensions.height = dh;

        // Grandchildren still need laying out, but this child's own outer
        // box is already fixed by the flex algorithm above - unlike a
        // block child, its size does NOT come from its own content, so
        // the returned content-height is intentionally discarded.
        let child_content_x = dx + p_left;
        let child_content_y = dy + p_top;
        let child_content_width = (dw - p_left - p_right).max(0.0);
        layout_children(child, child_content_width, child_content_x, child_content_y);

        main_cursor += m_main_start + padding_box_main + m_main_end + gap;
    }

    FlexResult {
        main_size: resolved_main,
        cross_size: resolved_cross,
    }
}
