//! Box-model edge/offset resolution helpers — split out from `layout.rs`.

use crate::style::{BorderStyle, Length};

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
pub(super) fn offset_from_edges(style: &crate::style::ComputedStyle) -> (f64, f64) {
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
pub(super) fn resolve_outer_width(
    style: &crate::style::ComputedStyle,
    containing_width: f64,
) -> f64 {
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
