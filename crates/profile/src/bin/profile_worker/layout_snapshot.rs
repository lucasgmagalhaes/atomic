//! Turning a computed `LayoutBox` tree into wire-shaped data for
//! `js_runtime`'s own real backing state: `getBoundingClientRect`/
//! `offsetWidth`/etc (`Context::set_layout_rects`) and `getComputedStyle`
//! (`Context::set_computed_styles`).

use std::collections::HashMap;

use dom::NodeId;
use js_runtime::Rect as LayoutMeasurementRect;
use layout_engine::{Color, Display, LayoutBox, Length, Position};

/// Flattens a laid-out `LayoutBox` tree into a `NodeId -> Rect` map for
/// `Context::set_layout_rects` — the real backing data for
/// `getBoundingClientRect`/`offsetWidth`/etc (see `js_runtime::
/// layout_measurement`). `LayoutBox::dimensions` is already absolute
/// (viewport-relative, unscrolled) document space, the same space
/// `render`'s own `build_display_list` paints from — no coordinate
/// conversion needed, just a straight copy per box. A synthetic inline-run
/// box (`LayoutBox::inline_spans`, see `layout_engine::tree`'s module doc)
/// still has a real `node` id (its first source child's) and dimensions,
/// so it's included like any other box, not skipped.
pub(crate) fn collect_layout_rects(tree: &LayoutBox) -> HashMap<NodeId, LayoutMeasurementRect> {
    fn walk(box_: &LayoutBox, out: &mut HashMap<NodeId, LayoutMeasurementRect>) {
        out.insert(
            box_.node,
            LayoutMeasurementRect {
                x: box_.dimensions.x,
                y: box_.dimensions.y,
                width: box_.dimensions.width,
                height: box_.dimensions.height,
            },
        );
        for child in &box_.children {
            walk(child, out);
        }
    }
    let mut out = HashMap::new();
    walk(tree, &mut out);
    out
}

/// Formats one `layout_engine::Length` the way real CSS would serialize a
/// resolved value: `px` for a resolved pixel length, `auto` for `Auto`, and
/// a plain percent string for `Percent` (real `getComputedStyle` resolves
/// percentages to pixels too, but this crate's `ComputedStyle` doesn't
/// carry the containing-block context needed to do that here — a
/// documented simplification, same shape as `layout-engine`'s own
/// `top`/`bottom` percentage scope cut).
fn format_length(length: Length) -> String {
    match length {
        Length::Px(px) => format!("{px}px"),
        Length::Percent(pct) => format!("{pct}%"),
        Length::Auto => "auto".to_string(),
    }
}

fn format_color(color: Color) -> String {
    format!(
        "rgba({}, {}, {}, {})",
        color.r,
        color.g,
        color.b,
        color.a as f64 / 255.0
    )
}

/// Flattens a laid-out `LayoutBox` tree's per-box `ComputedStyle` into a
/// `NodeId -> { property: value }` map for `Context::set_computed_styles`
/// — the real backing data for `getComputedStyle` (see `js_runtime::
/// computed_style`). Only the small, unambiguous-to-serialize property
/// subset below is included; `layout-engine`'s own `ComputedStyle` has
/// several more fields (`flex_grow`, `box_shadow`, ...) not attempted here
/// since a real caller almost never reads those through `getComputedStyle`
/// and this keeps the string-formatting surface bounded.
pub(crate) fn collect_computed_styles(
    tree: &LayoutBox,
) -> HashMap<NodeId, HashMap<String, String>> {
    fn walk(box_: &LayoutBox, out: &mut HashMap<NodeId, HashMap<String, String>>) {
        let style = &box_.style;
        let mut properties = HashMap::new();
        properties.insert(
            "display".to_string(),
            match style.display {
                Display::Block => "block",
                Display::Inline => "inline",
                Display::Flex => "flex",
                Display::None => "none",
            }
            .to_string(),
        );
        properties.insert(
            "position".to_string(),
            match style.position {
                Position::Static => "static",
                Position::Relative => "relative",
                Position::Absolute => "absolute",
            }
            .to_string(),
        );
        properties.insert("width".to_string(), format_length(style.width));
        properties.insert("height".to_string(), format_length(style.height));
        properties.insert("margin-top".to_string(), format_length(style.margin.top));
        properties.insert(
            "margin-right".to_string(),
            format_length(style.margin.right),
        );
        properties.insert(
            "margin-bottom".to_string(),
            format_length(style.margin.bottom),
        );
        properties.insert("margin-left".to_string(), format_length(style.margin.left));
        properties.insert("padding-top".to_string(), format_length(style.padding.top));
        properties.insert(
            "padding-right".to_string(),
            format_length(style.padding.right),
        );
        properties.insert(
            "padding-bottom".to_string(),
            format_length(style.padding.bottom),
        );
        properties.insert(
            "padding-left".to_string(),
            format_length(style.padding.left),
        );
        properties.insert("color".to_string(), format_color(style.color));
        properties.insert(
            "background-color".to_string(),
            format_color(style.background_color),
        );
        properties.insert("font-size".to_string(), format!("{}px", style.font_size));
        properties.insert("opacity".to_string(), style.opacity.to_string());
        out.insert(box_.node, properties);
        for child in &box_.children {
            walk(child, out);
        }
    }
    let mut out = HashMap::new();
    walk(tree, &mut out);
    out
}
