//! Flattens a `layout_engine::LayoutBox` tree into paint commands - a
//! "display list" in browser-engine terminology, decoupled from any GPU
//! API so it's testable without a device/adapter. Scoped to solid-color
//! rectangles (a box's padding box, filled with its `background_color`)
//! — no borders, no images, no text, no shadows, no clipping/scrolling.
use layout_engine::{Color, LayoutBox};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: Color,
}

/// Walks `box_` in paint order (parent before children, so later-painted
/// children correctly draw on top of their parent's background) and
/// collects one `Rect` per box with a non-transparent background.
/// Transparent boxes are skipped rather than emitted with a zero-alpha
/// rect — nothing downstream needs to know they exist.
pub fn build_display_list(box_: &LayoutBox) -> Vec<Rect> {
    let mut list = Vec::new();
    collect(box_, &mut list);
    list
}

fn collect(box_: &LayoutBox, out: &mut Vec<Rect>) {
    if box_.style.background_color.a > 0 {
        out.push(Rect {
            x: box_.dimensions.x as f32,
            y: box_.dimensions.y as f32,
            width: box_.dimensions.width as f32,
            height: box_.dimensions.height as f32,
            color: box_.style.background_color,
        });
    }
    for child in &box_.children {
        collect(child, out);
    }
}
