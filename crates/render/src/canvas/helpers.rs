//! Small pure-function helpers shared across `canvas`'s submodules -
//! color conversion, gradient math, coordinate mapping. No GPU/state
//! here, just the math every draw path needs.
use layout_engine::Color;

pub(super) fn color_to_f32(c: Color) -> [f32; 4] {
    [
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    ]
}

/// Pixel space (origin top-left, y-down) -> clip space (origin center,
/// y-up), the same conversion `pipeline::rect_vertices_colors` does
/// inline for each rect corner - factored out for `path::fill`'s path
/// triangulation, which has no fixed 4-corner shape to special-case.
pub(super) fn point_to_ndc(x: f32, y: f32, vw: f32, vh: f32) -> [f32; 2] {
    [(x / vw) * 2.0 - 1.0, 1.0 - (y / vh) * 2.0]
}

/// `from`/`to` at blend position `t` (clamped to `[0, 1]`) - plain
/// per-channel linear interpolation, straight (non-premultiplied) alpha,
/// same shape as `render::gpu::shader`'s own private `lerp_color` (not
/// reused directly - that one is private to its own module).
pub(super) fn lerp_color(from: Color, to: Color, t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) / 255.0;
    [
        mix(from.r, to.r),
        mix(from.g, to.g),
        mix(from.b, to.b),
        mix(from.a, to.a),
    ]
}

/// `t` (clamped `[0, 1]`) of point `(px, py)` projected onto the gradient
/// line `(x0, y0)`->`(x1, y1)` - the standard linear-gradient parametrization.
/// A zero-length line (`x0==x1 && y0==y1`) always returns `0.0` (the whole
/// fill becomes the gradient's start color) rather than dividing by zero.
pub(super) fn gradient_t(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return 0.0;
    }
    (((px - x0) * dx + (py - y0) * dy) / len_sq).clamp(0.0, 1.0)
}
