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
    pub(super) translate_x: f32,
    pub(super) translate_y: f32,
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
            translate_x: self.translate_x,
            translate_y: self.translate_y,
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
            self.translate_x = state.translate_x;
            self.translate_y = state.translate_y;
        }
    }

    /// `ctx.translate(x, y)` — offsets every subsequent `fillRect`/
    /// `clearRect`/`strokeRect` call by `(x, y)`, accumulating across
    /// repeated calls (real spec's own behavior: `translate` composes with
    /// the existing transform, it doesn't replace it). The one transform
    /// this crate supports - no `scale`/`rotate`/`setTransform`/general
    /// matrix, since none of those can be expressed as a plain coordinate
    /// offset the way translation can.
    pub fn translate(&mut self, x: f32, y: f32) {
        self.translate_x += x;
        self.translate_y += y;
    }
}
