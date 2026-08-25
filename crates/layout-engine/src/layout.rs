//! Layout orchestration: box-model resolution shared by every display type,
//! dispatching to block-stacking or flex for how children get arranged.
//! Block formatting context: children stack vertically, no inline flow, no
//! floats. `auto` margins resolve to `0.0`, not CSS's actual auto-margin
//! centering. No margin collapsing: adjacent margins both take full effect
//! instead of collapsing to the larger one.
//!
//! `position: relative`/`absolute` are real now (`static` is still every
//! box's default and behaves exactly as before) - see
//! `layout_block`/`layout_children`'s own docs for exactly what's
//! modeled. `fixed`/`sticky` aren't.
//!
//! `border-width`/`border-style`/`border-color` are real now too: a
//! bordered box's `border-width` genuinely grows `Dimensions` (real
//! box-model space, not just a paint-time decoration) — `Dimensions` is
//! the border box (content + padding + border) rather than the padding
//! box it used to be. `border-radius`/per-side border colors/styles
//! aren't modeled (see `style::ComputedStyle::border_color`'s own doc).
use crate::flex::layout_flex_children;
use crate::style::{BorderStyle, Clear, Display, Float, Length, Position};
use crate::text::{layout_inline, layout_text, InlineSpan};
use crate::tree::LayoutBox;

/// Lays out a text leaf box: measures/wraps it against `containing_width`
/// via `cosmic-text` and sets its dimensions/glyphs directly, bypassing
/// the normal box-model resolution entirely (a text box has no margin/
/// padding/explicit width/height to resolve from style - its size comes
/// purely from its shaped content). Returns the vertical space consumed,
/// same convention as `layout_block`.
fn layout_text_box(box_: &mut LayoutBox, containing_width: f64, x: f64, y: f64) -> f64 {
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
fn layout_inline_box(box_: &mut LayoutBox, containing_width: f64, x: f64, y: f64) -> f64 {
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

/// A single `top`/`right`/`bottom`/`left` offset value in pixels.
/// `Percent` falls back to `0.0` — see `style::ComputedStyle::top`'s own
/// doc on why a spec-correct resolution isn't attempted here.
fn resolve_offset(length: Length) -> f64 {
    match length {
        Length::Px(px) => px,
        Length::Percent(_) | Length::Auto => 0.0,
    }
}

/// Resolves `style`'s `top`/`right`/`bottom`/`left` into one `(dx, dy)`
/// pixel offset, real CSS's own precedence: `left` wins over `right`
/// (used as `-right` only when `left` is `Auto`), same for `top`/`bottom`.
/// `(0.0, 0.0)` if every edge is `Auto`.
///
/// Shared by both real uses `position` gets in this crate: for
/// `Relative`, the caller adds this to the box's already-flow-determined
/// `(x, y)` (a purely visual shift, see `layout_block`). For `Absolute`,
/// the caller uses this *as* the box's `(x, y)` directly instead of its
/// normal-flow position (see `layout_children`) — real in that an
/// absolutely positioned box genuinely leaves the flow and lands at a
/// real, independently-computed position, narrower than the spec in
/// exactly one way: it's always resolved against the page's own origin
/// `(0, 0)`, not the padding box of the nearest ancestor with
/// `position != static` (this crate's single top-down layout pass has no
/// "revisit once an ancestor's box is known" second pass to find one).
fn offset_from_edges(style: &crate::style::ComputedStyle) -> (f64, f64) {
    let dx = match style.left {
        Length::Auto => match style.right {
            Length::Auto => 0.0,
            other => -resolve_offset(other),
        },
        other => resolve_offset(other),
    };
    let dy = match style.top {
        Length::Auto => match style.bottom {
            Length::Auto => 0.0,
            other => -resolve_offset(other),
        },
        other => resolve_offset(other),
    };
    (dx, dy)
}

/// Resolves just a box's own outer (margin box) width from `style` and
/// `containing_width`, without laying out its subtree - mirrors exactly
/// the width math `layout_block` does internally (lines below), kept as a
/// separate pure function since a right-floated box's `x` depends on its
/// own resolved width, which normally isn't known until *after*
/// `layout_block` runs (see `layout_children`'s `Float::Right` handling).
/// Calling this first and then `layout_block` with the now-known `x`
/// means `layout_block` recomputes the identical width the normal way -
/// no float-specific branching needed inside it.
fn resolve_outer_width(style: &crate::style::ComputedStyle, containing_width: f64) -> f64 {
    let margin_left = resolve_edge(style.margin.left, containing_width);
    let margin_right = resolve_edge(style.margin.right, containing_width);
    let (border_left, border_right) = if style.border_style == BorderStyle::None {
        (0.0, 0.0)
    } else {
        (
            resolve_edge(style.border_width.left, containing_width),
            resolve_edge(style.border_width.right, containing_width),
        )
    };
    let padding_left = resolve_edge(style.padding.left, containing_width);
    let padding_right = resolve_edge(style.padding.right, containing_width);
    let content_width = match style.width {
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
    margin_left
        + margin_right
        + border_left
        + border_right
        + padding_left
        + padding_right
        + content_width
}

pub(crate) fn resolve_edge(length: Length, containing_width: f64) -> f64 {
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

/// Arranges `box_`'s direct children within its content box
/// (`content_width` × unbounded height, starting at `(content_x,
/// content_y)`) and returns the resulting content height - the space the
/// children actually occupy, before padding is added back on by the
/// caller. Dispatches on `box_.style.display`: `Flex` arranges children
/// via `flex::layout_flex_children`, everything else stacks them as block
/// boxes via `layout_block`.
///
/// Real `position: absolute` in the non-flex (block) branch: a child with
/// `Position::Absolute` is skipped by the normal stacking cursor entirely
/// (contributes `0` to the returned content height, doesn't push later
/// siblings down) and instead laid out via `layout_block` with
/// [`offset_from_edges`] used directly as its `(x, y)` — real removal
/// from flow, positioned independently. Scope cut: a `Flex` container's
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
    } else {
        let mut cursor_y = content_y;
        let mut left_edge_y = content_y;
        let mut right_edge_y = content_y;
        for child in &mut box_.children {
            if child.style.position == Position::Absolute {
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
