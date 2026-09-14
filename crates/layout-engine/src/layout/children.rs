//! `layout_children` — split out from `layout.rs`.

use crate::flex::layout_flex_children;
use crate::grid::layout_grid_children;
use crate::style::{Clear, Display, Float, Length, Position};
use crate::tree::LayoutBox;

use super::block::layout_block;
use super::offsets::{offset_from_edges, resolve_outer_width};

/// Arranges `box_`'s direct children within its content box
/// (`content_width` × unbounded height, starting at `(content_x,
/// content_y)`) and returns the resulting content height - the space the
/// children actually occupy, before padding is added back on by the
/// caller. Dispatches on `box_.style.display`: `Flex` arranges children
/// via `flex::layout_flex_children`, everything else stacks them as block
/// boxes via `layout_block`.
///
/// Real `position: absolute`/`position: fixed` in the non-flex (block)
/// branch: a child with either is skipped by the normal stacking cursor
/// entirely (contributes `0` to the returned content height, doesn't push
/// later siblings down) and instead laid out via `layout_block` with
/// [`offset_from_edges`] used directly as its `(x, y)` — real removal
/// from flow, positioned independently. `Fixed` gets identical layout math
/// to `Absolute` (both already resolve against the page's own origin, not
/// a positioned ancestor - see [`offset_from_edges`]'s own doc); the real
/// difference between them is paint-time only, in `render`/`profile-
/// worker`'s page-scroll shift - see `style::Position`'s own doc. Scope
/// cut: a `Flex` container's
/// own children never get this treatment (an absolutely positioned flex
/// item still participates in flex sizing/placement as if it were a
/// normal item) - the flex algorithm has no equivalent "skip me" path,
/// and flex+abspos together is enough of a corner case that adding one
/// wasn't worth it for this pass.
///
/// Real `float`/`clear` in the same branch, same "real but narrower than
/// spec" treatment `position` already got (see [`crate::style::Float`]'s
/// own doc for exactly what's cut): `left_edge_y`/`right_edge_y` track
/// the lowest point reached by floats on each side so far, seeded at
/// `content_y`. A floated child is removed from the normal stacking
/// cursor (like `absolute`) and placed flush to its side's edge at
/// `max(cursor_y, side_edge_y)` — never above the general flow position
/// it appears at in document order, and never overlapping an earlier
/// same-side float — then that side's edge advances by the float's own
/// total height (`layout_block`'s return value). A non-floated child with
/// `clear` pushes `cursor_y` down to the named side's edge (or both)
/// before it's laid out, same real "below every earlier float on that
/// side" behavior the spec gives `clear`. What's real but not spec-exact:
/// no line-box narrowing (nothing here makes inline content wrap around a
/// float — a float just visually sits wherever it's placed, independent
/// of any inline sibling), and this container's own returned height
/// always grows to include its floats' full extent (`cursor_y`/
/// `left_edge_y`/`right_edge_y`, whichever is greatest) — real CSS only
/// does that for a container establishing its own block formatting
/// context (e.g. `overflow` other than `visible`), not every block
/// unconditionally; tracking BFC establishment wasn't worth adding for
/// this pass, and "always contains its floats" is arguably more useful
/// anyway absent a `clearfix`-equivalent a page could reach for.
pub(crate) fn layout_children(
    box_: &mut LayoutBox,
    content_width: f64,
    content_x: f64,
    content_y: f64,
) -> f64 {
    if box_.style.display == Display::Flex {
        let explicit_height = match box_.style.height {
            Length::Px(px) => Some(px),
            _ => None,
        };
        let is_row = box_.style.flex_direction == crate::style::FlexDirection::Row;
        let (main_known, cross_known) = if is_row {
            (Some(content_width), explicit_height)
        } else {
            (explicit_height, Some(content_width))
        };
        let result = layout_flex_children(box_, main_known, cross_known, content_x, content_y);
        if is_row {
            result.cross_size
        } else {
            result.main_size
        }
    } else if box_.style.display == Display::Grid {
        let result = layout_grid_children(box_, content_width, content_x, content_y);
        result.height
    } else {
        let mut cursor_y = content_y;
        let mut left_edge_y = content_y;
        let mut right_edge_y = content_y;
        for child in &mut box_.children {
            if child.style.position == Position::Absolute || child.style.position == Position::Fixed
            {
                let (dx, dy) = offset_from_edges(&child.style);
                layout_block(child, content_width, dx, dy);
                continue;
            }
            if child.style.float != Float::None {
                let float_y = cursor_y.max(if child.style.float == Float::Left {
                    left_edge_y
                } else {
                    right_edge_y
                });
                let float_x = if child.style.float == Float::Left {
                    content_x
                } else {
                    let outer_width = resolve_outer_width(&child.style, content_width);
                    content_x + (content_width - outer_width).max(0.0)
                };
                let total_height = layout_block(child, content_width, float_x, float_y);
                if child.style.float == Float::Left {
                    left_edge_y = float_y + total_height;
                } else {
                    right_edge_y = float_y + total_height;
                }
                continue;
            }
            if child.style.clear == Clear::Left || child.style.clear == Clear::Both {
                cursor_y = cursor_y.max(left_edge_y);
            }
            if child.style.clear == Clear::Right || child.style.clear == Clear::Both {
                cursor_y = cursor_y.max(right_edge_y);
            }
            cursor_y += layout_block(child, content_width, content_x, cursor_y);
        }
        (cursor_y - content_y)
            .max(left_edge_y - content_y)
            .max(right_edge_y - content_y)
    }
}
