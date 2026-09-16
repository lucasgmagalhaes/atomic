//! CSS Grid layout (`ROADMAP.md` item 34): single-level, fixed-track
//! grid with row-major auto-placement — no shared-infrastructure gap
//! blocked this (see `.claude/plans/architecture-p5-foundations.plan.md`'s
//! own note that item 34 needed no restructuring), so this mirrors
//! `flex.rs`'s existing "real but narrower than spec" shape directly.
//!
//! What's real: `grid-template-columns`/`grid-template-rows` with `px`
//! and `fr` tracks (see [`crate::style::GridTrackSize`]), and
//! auto-placement of children into cells in document order (row-major,
//! wrapping to a new row once every column in the current row is
//! filled). Each item is laid out via [`crate::layout::layout_block`]
//! with its cell's width as the containing width — a `width: auto` item
//! (the common case) naturally fills its column, matching Grid's own
//! default `justify-items: stretch` for free, no separate override
//! needed.
//!
//! What's cut, deliberately, same "real but narrower" convention
//! `flex.rs`'s own module doc uses for its missing `flex-wrap`/`order`/
//! `gap`:
//! - No vertical stretch: an item shorter than its row is top-aligned
//!   within it, not grown to fill the row's full height (real Grid's
//!   default `align-items: stretch` would do that) — doing so needs a
//!   *forced* height independent of the item's own resolved height, a
//!   concept `layout_block` doesn't have (it always resolves height from
//!   the item's own CSS/content, the same way every other box in this
//!   engine does); adding one just for this pass wasn't worth it.
//! - No `grid-column`/`grid-row` placement or spanning — every item
//!   occupies exactly one auto-placed cell.
//! - No `grid-template-areas`, no named lines, no `repeat()`/`minmax()`/
//!   `auto` tracks (see [`crate::style::GridTrackSize`]'s own doc).
//! - No `gap`/`row-gap`/`column-gap` — this crate has no `gap` property
//!   on `ComputedStyle` at all yet (`flex` doesn't either).
//! - Rows have no `fr` sizing: a `grid-template-rows` track's `Fr` variant
//!   is treated as `auto` (falls back to content height) since this
//!   engine's top-down layout pass has no definite container height to
//!   distribute rows against, the same reason `flex.rs`'s own
//!   `cross_of` falls back to `0.0`/auto sizing when a container's
//!   cross size isn't known.
//! - No explicit `grid-template-columns`/`-rows`: a container with none
//!   falls back to a single full-width column (so `display: grid` with
//!   no track list still lays its children out, one per row, rather than
//!   collapsing everything into one cell or panicking on an empty list).
//!
//! Row heights are unknown until every item in a row has been measured,
//! so (unlike `flex`, which never needs a pre-pass) this does two real
//! `layout_block` passes per item: one to measure its natural height at
//! its cell's width, one to place it for real once every row's height is
//! known — real engines do multi-pass sizing for intrinsic grid sizing
//! too, this just does the minimum version of that.

use crate::layout::layout_block;
use crate::style::GridTrackSize;
use crate::tree::LayoutBox;

/// Resolves a column track list against `content_width`: every `Px`
/// track keeps its own size; the width left over after subtracting all
/// `Px` tracks is distributed across `Fr` tracks proportional to their
/// own share (a `2fr` track gets twice a `1fr` track's share of the
/// leftover) — real CSS Grid's own `fr` distribution, restricted to this
/// single-pass shape (no `minmax()` floor to satisfy first).
fn resolve_column_tracks(tracks: &[GridTrackSize], content_width: f64) -> Vec<f64> {
    let fixed_total: f64 = tracks
        .iter()
        .map(|t| match t {
            GridTrackSize::Px(px) => *px,
            GridTrackSize::Fr(_) => 0.0,
        })
        .sum();
    let fr_total: f64 = tracks
        .iter()
        .map(|t| match t {
            GridTrackSize::Fr(fr) => *fr,
            GridTrackSize::Px(_) => 0.0,
        })
        .sum();
    let leftover = (content_width - fixed_total).max(0.0);
    tracks
        .iter()
        .map(|t| match t {
            GridTrackSize::Px(px) => *px,
            GridTrackSize::Fr(fr) if fr_total > 0.0 => leftover * (fr / fr_total),
            GridTrackSize::Fr(_) => 0.0,
        })
        .collect()
}

pub struct GridResult {
    pub width: f64,
    pub height: f64,
}

/// Lays out `container`'s direct children as grid items and recurses
/// into each child's own subtree (via `layout_block`, which already
/// calls `layout_children` for grandchildren) — see the module doc for
/// the two-pass shape.
pub fn layout_grid_children(
    container: &mut LayoutBox,
    content_width: f64,
    content_x: f64,
    content_y: f64,
) -> GridResult {
    // Collected into owned `Vec`s (both are tiny, capped at
    // `MAX_GRID_TRACKS`) so the borrow of `container.style` doesn't
    // outlive the `container.children` mutation below.
    let column_tracks: Vec<GridTrackSize> =
        container.style.grid_template_columns.as_slice().to_vec();
    let row_tracks: Vec<GridTrackSize> = container.style.grid_template_rows.as_slice().to_vec();

    let column_widths = if column_tracks.is_empty() {
        vec![content_width]
    } else {
        resolve_column_tracks(&column_tracks, content_width)
    };
    let num_columns = column_widths.len().max(1);

    let mut column_x = Vec::with_capacity(num_columns);
    let mut cursor = content_x;
    for &w in &column_widths {
        column_x.push(cursor);
        cursor += w;
    }

    // Pass 1: measure each item's natural outer height at its cell's
    // width, at a placeholder origin — only the height is used from
    // this pass, so its position/subtree layout is redone in pass 2.
    let mut item_heights = Vec::with_capacity(container.children.len());
    for (i, child) in container.children.iter_mut().enumerate() {
        let col = i % num_columns;
        item_heights.push(layout_block(child, column_widths[col], 0.0, 0.0));
    }

    let num_rows = container.children.len().div_ceil(num_columns).max(1);
    let mut row_heights = Vec::with_capacity(num_rows);
    for row in 0..num_rows {
        let explicit = row_tracks.get(row).and_then(|t| match t {
            GridTrackSize::Px(px) => Some(*px),
            GridTrackSize::Fr(_) => None,
        });
        let auto_height = (0..num_columns)
            .filter_map(|col| item_heights.get(row * num_columns + col))
            .cloned()
            .fold(0.0_f64, f64::max);
        row_heights.push(explicit.unwrap_or(auto_height));
    }

    let mut row_y = Vec::with_capacity(num_rows);
    let mut cursor_y = content_y;
    for &h in &row_heights {
        row_y.push(cursor_y);
        cursor_y += h;
    }
    let total_height = cursor_y - content_y;
    let total_width: f64 = column_widths.iter().sum();

    // Pass 2: real placement at each item's final cell origin.
    for (i, child) in container.children.iter_mut().enumerate() {
        let col = i % num_columns;
        let row = i / num_columns;
        layout_block(child, column_widths[col], column_x[col], row_y[row]);
    }

    GridResult {
        width: total_width,
        height: total_height,
    }
}
