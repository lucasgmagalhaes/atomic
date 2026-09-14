//! CSS table layout (`spec/matrix/css-layout.md`'s "full formatting
//! contexts" line): a real `display: table` / `table-row` / `table-cell`
//! three-level box structure, laid out top-down like every other box in
//! this crate — no shared-infrastructure gap blocked this, same as
//! `grid.rs`'s own note, so this mirrors that module's exact shape:
//! `LayoutBox` fields, a two-pass measure-then-place algorithm (row
//! heights aren't known until every cell in a row has been measured), and
//! the same "real but narrower than spec" documentation convention.
//!
//! What's real: any `display: table` box's direct children, each treated
//! as a row (regardless of their own `display` — see below), and each
//! row's own direct children, each treated as a cell. Column count is the
//! widest row's own child count. Every cell in a row shares that row's own
//! height (the tallest cell measured at its column's width). Cells paint
//! and hit-test individually via the normal `layout_block` recursion, so
//! backgrounds/borders/text/nested elements inside a cell are all real.
//!
//! What's cut, deliberately:
//! - **Equal-width columns only** — real CSS's column-width algorithm
//!   (content-based intrinsic sizing, `<col>`/`<colgroup>` widths,
//!   `table-layout: fixed` vs `auto`) isn't modeled; every column gets
//!   `content_width / num_columns`, the same "real but simplified"
//!   distribution `grid.rs`'s own missing `minmax()`/content-sizing cut
//!   already takes for grid tracks.
//! - **No anonymous box generation.** Real CSS wraps a bare `<td>` with no
//!   `<tr>` parent (or a `<tr>` with no `<table>` parent) in implied
//!   anonymous table boxes; this crate doesn't build any HTML-tag-to-
//!   `display` default table at all (see `tree::build`'s own doc on that
//!   same gap for `display: inline`), so a `display: table` box's
//!   children are simply always treated as rows and grandchildren as
//!   cells, real HTML tag or not — matching how a page's stylesheet must
//!   already say `b, span { display: inline; }` explicitly for inline
//!   flow to work in this engine.
//! - **No `colspan`/`rowspan`.** Every cell occupies exactly one column of
//!   one row — same "no spanning" scope cut `grid.rs`'s own missing
//!   `grid-column`/`grid-row` placement already takes.
//! - **No `border-collapse`, no `table-row-group`/`thead`/`tbody`/`tfoot`
//!   as a distinct row-grouping level** — a `display: table` box's
//!   children are always rows directly, one level, not two.
//! - **No vertical cell alignment** (`vertical-align` isn't modeled
//!   anywhere in this crate yet) — a cell shorter than its row is top-
//!   aligned within it, same "no stretch" cut `grid.rs`'s own missing
//!   `align-items: stretch` already takes.
//! - **No `gap`/`border-spacing`** — this crate has no `gap` property on
//!   `ComputedStyle` at all yet (neither `flex` nor `grid` have it
//!   either).

use crate::layout::layout_block;
use crate::tree::{Dimensions, LayoutBox};

pub struct TableResult {
    pub width: f64,
    pub height: f64,
}

/// Lays out `container`'s children as table rows (and each row's own
/// children as cells) — see the module doc for the two-pass shape and
/// every scope cut. A table with no rows, or every row empty, contributes
/// zero height and the full `content_width`, same "empty but valid"
/// convention `layout_grid_children` already has for zero children.
pub fn layout_table_children(
    container: &mut LayoutBox,
    content_width: f64,
    content_x: f64,
    content_y: f64,
) -> TableResult {
    let num_columns = container
        .children
        .iter()
        .map(|row| row.children.len())
        .max()
        .unwrap_or(0)
        .max(1);
    let column_width = content_width / num_columns as f64;

    // Pass 1: measure every cell's natural height at its column's width,
    // at a placeholder origin — only the height is used from this pass,
    // its real position/subtree layout is redone in pass 2.
    let mut row_heights = Vec::with_capacity(container.children.len());
    for row in &mut container.children {
        let mut row_height = 0.0_f64;
        for cell in &mut row.children {
            let h = layout_block(cell, column_width, 0.0, 0.0);
            row_height = row_height.max(h);
        }
        row_heights.push(row_height);
    }

    // Pass 2: real placement. A row itself isn't a real block box in this
    // model (its layout is entirely governed by this function, not
    // `layout_children`'s normal recursion — real spec's own "table rows
    // aren't laid out like ordinary blocks" rule) — its `Dimensions` are
    // set directly rather than via `layout_block`, so its own background/
    // border (real properties a page can still set on `display: table-row`
    // for e.g. zebra striping) paint at the right place.
    let mut cursor_y = content_y;
    for (row, &row_height) in container.children.iter_mut().zip(row_heights.iter()) {
        row.dimensions = Dimensions {
            x: content_x,
            y: cursor_y,
            width: content_width,
            height: row_height,
        };
        let mut cursor_x = content_x;
        for cell in &mut row.children {
            layout_block(cell, column_width, cursor_x, cursor_y);
            cursor_x += column_width;
        }
        cursor_y += row_height;
    }

    TableResult {
        width: content_width,
        height: cursor_y - content_y,
    }
}
