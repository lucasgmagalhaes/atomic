//! Splits a container rect into cells for the mockup's pane-count toggle
//! (1/2/4/6 - see `mockup/browser-idle-spec.md`'s feature-mapping table,
//! "Grid de panes 1/2/4/6 com tiling"). Pure geometry, no `egui` dependency
//! - directly testable, and reused by `main.rs` only to convert into
//! `egui::Rect`s for drawing.
//!
//! Naming note: the spec table calls this "layout BSP" (binary space
//! partitioning). What's here isn't that — it's four fixed presets (a
//! recursive BSP split naturally produces uneven strips for a count like 4
//! or 6, not the even 2×2/2×3 grid the mockup's toggle actually shows).
//! Any count outside {1, 2, 4, 6} falls back to a single row of equal
//! columns rather than refusing to lay out at all.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Lays out `pane_count` equal-ish cells inside `container`:
/// - 0 -> no cells
/// - 1 -> the whole container
/// - 2 -> two side-by-side columns
/// - 4 -> a 2×2 grid
/// - 6 -> a 2-row × 3-column grid
/// - anything else -> `pane_count` equal columns in a single row
pub fn grid_layout(container: Rect, pane_count: usize) -> Vec<Rect> {
    match pane_count {
        0 => Vec::new(),
        1 => vec![container],
        4 => rows_of_columns(container, 2, 2),
        6 => rows_of_columns(container, 2, 3),
        n => columns(container, n),
    }
}

fn columns(container: Rect, n: usize) -> Vec<Rect> {
    let width = container.width / n as f32;
    (0..n)
        .map(|i| Rect {
            x: container.x + width * i as f32,
            y: container.y,
            width,
            height: container.height,
        })
        .collect()
}

fn rows_of_columns(container: Rect, rows: usize, cols: usize) -> Vec<Rect> {
    let width = container.width / cols as f32;
    let height = container.height / rows as f32;
    let mut out = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            out.push(Rect {
                x: container.x + width * c as f32,
                y: container.y + height * r as f32,
                width,
                height,
            });
        }
    }
    out
}
