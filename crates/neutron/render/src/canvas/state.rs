//! `save`/`restore`'s state stack, plus the plain scalar drawing-state
//! setters/getters (`fillStyle`/`strokeStyle`/`lineWidth`/`translate`) -
//! split out from `mod.rs` since gradient (`gradients.rs`), font
//! (`text.rs`), and path (`path.rs`) state each have their own file; this
//! is what's left once those are carved out.
use layout_engine::{Color, FontFamily};

use super::gradients::FillGradient;
use super::Canvas2D;

#[derive(Clone, Copy)]
pub(super) struct CanvasState {
    pub(super) fill_style: Color,
    pub(super) fill_gradient: Option<FillGradient>,
    pub(super) stroke_style: Color,
    pub(super) line_width: f32,
    pub(super) font_size: f32,
    pub(super) font_family: FontFamily,
    pub(super) transform: [f32; 6],
}

impl Canvas2D {
    pub fn set_fill_style(&mut self, color: Color) {
        self.fill_style = color;
        self.fill_gradient = None;
    }

    /// `ctx.fillStyle`'s getter side — a JS binding (`js-runtime`'s
    /// `canvas_bindings`) formats this back to a `#rrggbb` hex string.
    pub fn fill_style(&self) -> Color {
        self.fill_style
    }

    pub fn set_stroke_style(&mut self, color: Color) {
        self.stroke_style = color;
    }

    /// `ctx.strokeStyle`'s getter side, same shape as [`Canvas2D::fill_style`].
    pub fn stroke_style(&self) -> Color {
        self.stroke_style
    }

    pub fn set_line_width(&mut self, width: f32) {
        self.line_width = width;
    }

    /// `ctx.lineWidth`'s getter side.
    pub fn line_width(&self) -> f32 {
        self.line_width
    }

    /// `ctx.save()` — pushes the current drawing state onto
    /// `Canvas2D::state_stack`.
    pub fn save(&mut self) {
        self.state_stack.push(CanvasState {
            fill_style: self.fill_style,
            fill_gradient: self.fill_gradient,
            stroke_style: self.stroke_style,
            line_width: self.line_width,
            font_size: self.font_size,
            font_family: self.font_family,
            transform: self.transform,
        });
    }

    /// `ctx.restore()` — pops and applies the most recently saved drawing
    /// state. A no-op on an empty stack (matches real spec: calling
    /// `restore()` with nothing left to restore does nothing, not error).
    pub fn restore(&mut self) {
        if let Some(state) = self.state_stack.pop() {
            self.fill_style = state.fill_style;
            self.fill_gradient = state.fill_gradient;
            self.stroke_style = state.stroke_style;
            self.line_width = state.line_width;
            self.font_size = state.font_size;
            self.font_family = state.font_family;
            self.transform = state.transform;
        }
    }

    /// Composes the current transform matrix with the 2x2-linear-plus-
    /// translate op `(a2, b2, c2, d2, e2, f2)` on the right —
    /// `self.transform = self.transform * op`, the standard CTM
    /// composition every one of `translate`/`scale`/`rotate` reduces to.
    /// Matches real spec's "applied on top of the current transform"
    /// semantics: a new op happens in the canvas's *current local*
    /// coordinate system, not the original untransformed one.
    fn compose(&mut self, a2: f32, b2: f32, c2: f32, d2: f32, e2: f32, f2: f32) {
        let [a, b, c, d, e, f] = self.transform;
        self.transform = [
            a * a2 + c * b2,
            b * a2 + d * b2,
            a * c2 + c * d2,
            b * c2 + d * d2,
            a * e2 + c * f2 + e,
            b * e2 + d * f2 + f,
        ];
    }

    /// `ctx.translate(x, y)` — offsets every subsequent draw call,
    /// accumulating/composing with any prior `translate`/`scale`/`rotate`
    /// (real spec's own behavior: each call composes with the existing
    /// transform, it doesn't replace it).
    pub fn translate(&mut self, x: f32, y: f32) {
        self.compose(1.0, 0.0, 0.0, 1.0, x, y);
    }

    /// `ctx.scale(x, y)` — scales every subsequent draw call's
    /// coordinates, composed with the current transform same as
    /// [`Self::translate`].
    pub fn scale(&mut self, x: f32, y: f32) {
        self.compose(x, 0.0, 0.0, y, 0.0, 0.0);
    }

    /// `ctx.rotate(angle)` — rotates (radians, clockwise in this crate's
    /// y-down pixel space, matching real spec) every subsequent draw
    /// call's coordinates, composed with the current transform same as
    /// [`Self::translate`].
    pub fn rotate(&mut self, angle: f32) {
        let (s, c) = angle.sin_cos();
        self.compose(c, s, -s, c, 0.0, 0.0);
    }

    /// `ctx.setTransform(a, b, c, d, e, f)` — **replaces** the current
    /// transform outright (real spec: unlike `translate`/`scale`/
    /// `rotate`, this does not compose with what was there before).
    pub fn set_transform(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
        self.transform = [a, b, c, d, e, f];
    }

    /// `ctx.resetTransform()` — sets the transform back to the identity
    /// matrix.
    pub fn reset_transform(&mut self) {
        self.transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    }

    /// Applies the current transform matrix to a point — every draw
    /// path (`shapes`/`gradients`/`path`/`text`) funnels its coordinates
    /// through this instead of the old plain `translate_x`/`translate_y`
    /// offset add.
    pub(super) fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        let [a, b, c, d, e, f] = self.transform;
        (a * x + c * y + e, b * x + d * y + f)
    }

    /// The 4 corners of the axis-aligned rect `(x, y, w, h)` in *local*
    /// canvas space, each independently run through
    /// [`Self::transform_point`] - `[tl, tr, bl, br]`, matching
    /// `pipeline::rect_vertices`'s expected corner order. A `scale`/
    /// `rotate` in the current transform turns this into a genuine
    /// non-axis-aligned quad, not just an offset rect - each corner needs
    /// its own transform, not one shared translate.
    pub(super) fn transformed_corners(&self, x: f32, y: f32, w: f32, h: f32) -> [(f32, f32); 4] {
        [
            self.transform_point(x, y),
            self.transform_point(x + w, y),
            self.transform_point(x, y + h),
            self.transform_point(x + w, y + h),
        ]
    }
}
