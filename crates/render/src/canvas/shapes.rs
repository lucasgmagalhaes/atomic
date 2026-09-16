//! Solid-color rectangle drawing - `fillRect`/`clearRect`/`strokeRect`
//! and the shared `draw_rect`/`submit_vertices` plumbing they funnel
//! through. Gradient fills live in `gradients.rs`; this file only
//! handles the plain `fillStyle`/`strokeStyle` color case.
use layout_engine::Color;

use super::helpers::color_to_f32;
use super::pipeline::{rect_vertices, Vertex};
use super::Canvas2D;

impl Canvas2D {
    /// Applies `translate_x`/`translate_y` to every rect this crate paints -
    /// `fill_rect`/`clear_rect`/`stroke_rect` all funnel through here, so
    /// this is the one place the translate offset needs to be added.
    fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color, replace: bool) {
        let (x, y) = (x + self.translate_x, y + self.translate_y);
        let vertices = rect_vertices(
            x,
            y,
            w,
            h,
            color_to_f32(color),
            self.width as f32,
            self.height as f32,
        );
        self.submit_vertices(&vertices, replace);
    }

    /// Submits an arbitrary triangle-list vertex buffer (`vertices.len()`
    /// must be a multiple of 3) - rects always pass exactly 6 (two
    /// triangles), a filled path (`path::fill`) passes
    /// `(point_count - 2) * 3` from its fan triangulation. `pub(super)`
    /// since `path.rs` and `gradients.rs` (the linear-gradient case) both
    /// call this too.
    pub(super) fn submit_vertices(&mut self, vertices: &[Vertex], replace: bool) {
        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        use wgpu::util::DeviceExt;
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("canvas2d-rect-vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas2d-draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    // Load, not Clear: this canvas accumulates draws across
                    // calls, unlike GpuRenderer's one-shot render_to_rgba.
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(if replace {
                &self.clear_pipeline
            } else {
                &self.fill_pipeline
            });
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// `ctx.fillRect(x, y, w, h)` using the current `fillStyle` - a
    /// gradient (see `Canvas2D::set_fill_gradient`) if one is set, else
    /// the plain `fill_style` color.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        match self.fill_gradient {
            Some(super::FillGradient::Linear(gradient)) => {
                self.draw_gradient_rect(x, y, w, h, gradient)
            }
            Some(super::FillGradient::Radial(gradient)) => {
                self.draw_radial_gradient_rect(x, y, w, h, gradient)
            }
            None => self.draw_rect(x, y, w, h, self.fill_style, false),
        }
    }

    /// `ctx.clearRect(x, y, w, h)` — always transparent, regardless of
    /// `fillStyle`, and always a hard replace (see `clear_pipeline`).
    pub fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.draw_rect(x, y, w, h, Color::TRANSPARENT, true);
    }

    /// `ctx.strokeRect(x, y, w, h)` — draws the rectangle's outline only,
    /// using the current `strokeStyle`/`lineWidth`, as four filled bars (one
    /// per side) each centered on that edge - matching real Canvas2D's own
    /// "stroke straddles the path" positioning, not drawn fully inside or
    /// outside the rect. Scoped to axis-aligned rectangles only, since
    /// there is no general path/line-join machinery here (no miter/bevel/
    /// round joins) - the four bars simply overlap at each corner, which is
    /// visually correct for an opaque `strokeStyle` but would double-blend
    /// a semi-transparent one.
    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let lw = self.line_width;
        let half = lw / 2.0;
        let color = self.stroke_style;
        self.draw_rect(x - half, y - half, w + lw, lw, color, false);
        self.draw_rect(x - half, y + h - half, w + lw, lw, color, false);
        self.draw_rect(x - half, y - half, lw, h + lw, color, false);
        self.draw_rect(x + w - half, y - half, lw, h + lw, color, false);
    }
}
