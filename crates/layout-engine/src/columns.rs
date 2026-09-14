//! CSS multicolumn layout (`spec/matrix/css-layout.md`'s "full formatting
//! contexts" line): real `column-count`, splitting a block container's
//! direct children across N equal-width side-by-side columns.
//!
//! What's real: `column-count: N` on any block container (not gated on
//! `display` at all — real CSS's own rule, unlike `grid`/`table`/`flex`,
//! which *are* `display` keywords) splits `container.children` into `N`
//! chunks in document order (`children.len().div_ceil(N)` per column,
//! same chunking shape `table.rs`'s own row/column split already uses
//! elsewhere in this crate) and stacks each chunk vertically within its
//! own column, columns placed side by side left to right.
//!
//! What's cut, deliberately:
//! - **Count-based chunking, not real height balancing.** Real CSS
//!   multicol measures total content height first, then balances columns
//!   so each is roughly the same *height* (the last column often shorter
//!   than the others). This crate has no such measure-then-rebalance pass
//!   — children are simply split by *count* into equal-sized chunks, the
//!   same "real but simplified" scope cut `table.rs`'s own missing
//!   content-based column-width algorithm already takes. A page with
//!   very unevenly sized children will see uneven column heights.
//! - **No `column-width`, no `columns` shorthand, no `column-gap`** (this
//!   crate has no `gap` property on `ComputedStyle` at all yet — neither
//!   `flex` nor `grid` have it either), **no `column-rule`**, **no
//!   spanning content** (`column-span: all`).
//! - **No cross-column element splitting/fragmentation** — an element
//!   that would overflow its own column's available height just extends
//!   past it (this crate has no fragmentation anywhere, same real gap
//!   `spec/matrix/css-layout.md`'s own "fragmentation" line already
//!   tracks) rather than being split across two columns.

use crate::layout::layout_block;
use crate::tree::LayoutBox;

pub struct ColumnsResult {
    pub width: f64,
    pub height: f64,
}

/// Lays out `container`'s children split across `column_count` columns —
/// see the module doc for the exact chunking shape and every scope cut.
/// Zero children contributes zero height, same "empty but valid"
/// convention `table.rs`/`grid.rs` already have.
pub fn layout_column_children(
    container: &mut LayoutBox,
    column_count: u32,
    content_width: f64,
    content_x: f64,
    content_y: f64,
) -> ColumnsResult {
    let column_count = column_count.max(1) as usize;
    let column_width = content_width / column_count as f64;
    let per_column = container.children.len().div_ceil(column_count).max(1);

    let mut max_column_height = 0.0_f64;
    for (col, chunk) in container.children.chunks_mut(per_column).enumerate() {
        let column_x = content_x + col as f64 * column_width;
        let mut cursor_y = content_y;
        for child in chunk.iter_mut() {
            cursor_y += layout_block(child, column_width, column_x, cursor_y);
        }
        max_column_height = max_column_height.max(cursor_y - content_y);
    }

    ColumnsResult {
        width: content_width,
        height: max_column_height,
    }
}
