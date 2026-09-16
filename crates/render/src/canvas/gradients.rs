//! `fillStyle` gradients — linear (reuses the solid pipeline's
//! 4-corner-color trick) and radial (a dedicated exact per-pixel shader,
//! see [`RadialGradient`]'s own doc for why). Split out from `mod.rs`.
use layout_engine::Color;

use super::helpers::{color_to_f32, gradient_t, lerp_color, point_to_ndc};
use super::pipeline::{rect_vertices_colors, RadialVertex};
use super::Canvas2D;

/// `ctx.createLinearGradient(x0, y0, x1, y1)` + two `addColorStop` calls -
/// scoped to exactly two effective color stops (the start and end color),
/// not spec's arbitrary N-stop list. A 3rd+ `addColorStop` call is
/// accepted (real spec allows any offset) but only ever overwrites
/// whichever of `start`/`end` its offset is closer to - no internal color
/// stops, since exact multi-stop interpolation across a rect would need
/// more than the 4-corner GPU-interpolation trick this reuses from
/// `render::gpu::shader`'s own 2-stop `linear-gradient` background support
/// (see that module's comment on why the 2-stop case specifically is
/// exact, not an approximation). Coordinates are plain canvas pixel space,
/// unaffected by `ctx.translate()` - a documented scope cut (translate
/// only shifts where the *filled shape* lands, not where the gradient's
/// own line sits).
#[derive(Clone, Copy)]
pub struct LinearGradient {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub start: Color,
    pub end: Color,
}

/// `ctx.createRadialGradient(x0, y0, r0, x1, y1, r1)` — scoped to a single
/// circle: only the *outer* circle (`x1`, `y1`, `r1`) is used, matching
/// [`LinearGradient`]'s own "2 effective stops" cut in spirit - real
/// spec's two-circle cone-gradient shape (a genuinely different visual
/// when the inner/outer circles don't share a center) isn't modeled, only
/// the common "expanding circle from a center point" case. Unlike
/// [`LinearGradient`] (which reuses the existing solid-color pipeline's
/// 4-corner-interpolation trick - exact only because a linear function is
/// exactly barycentric-interpolable), a radial gradient's `t =
/// distance/radius` is **not** affine in screen position, so that same
/// trick would only be exact at the rect's own corners and visibly wrong
/// (a diamond-shaped blend, not a circle) everywhere else. Instead this
/// gets its own tiny GPU pipeline (`pipeline::RADIAL_SHADER_SRC`):
/// `local_pos` (position relative to the gradient center) is itself an
/// affine function of screen position, so it interpolates exactly across
/// the rect, and the nonlinear `length(local_pos) / radius` clamp-and-lerp
/// happens per-fragment in `fs_main` - a real, exact circular gradient,
/// not an approximation, same standard as the linear case.
#[derive(Clone, Copy)]
pub struct RadialGradient {
    pub cx: f32,
    pub cy: f32,
    pub radius: f32,
    pub start: Color,
    pub end: Color,
}

/// `ctx.fillStyle = gradient`'s two real shapes - see [`LinearGradient`]
/// and [`RadialGradient`]'s own docs for what's real vs cut in each.
#[derive(Clone, Copy)]
pub enum FillGradient {
    Linear(LinearGradient),
    Radial(RadialGradient),
}

impl Canvas2D {
    /// `ctx.fillStyle = gradient` — sets a [`FillGradient`] (linear or
    /// radial) as the fill paint for subsequent `fill_rect` calls, without
    /// touching the plain `fill_style` color underneath (so a later
    /// `set_fill_style` still has something sane to fall back to, and
    /// `fill_style()`'s own getter keeps returning that last solid color -
    /// see its own doc for why this crate doesn't round-trip the actual
    /// gradient object back out).
    pub fn set_fill_gradient(&mut self, gradient: FillGradient) {
        self.fill_gradient = Some(gradient);
    }

    /// `Some` when the current fill paint is a gradient, not a plain
    /// `fill_style` color.
    pub fn fill_gradient(&self) -> Option<FillGradient> {
        self.fill_gradient
    }

    /// Same as `shapes`'s private `draw_rect` but with a [`LinearGradient`]
    /// fill instead of a solid color - computes each corner's exact color
    /// by projecting it onto the gradient line (see `helpers::gradient_t`),
    /// then lets the GPU's own vertex-color interpolation fill in the
    /// interior. Gradient coordinates are *not* offset by
    /// `translate_x`/`translate_y` (see [`LinearGradient`]'s own doc on
    /// this scope cut) - only the rect's own position is. `pub(super)`
    /// since `shapes::fill_rect` dispatches to this.
    pub(super) fn draw_gradient_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        gradient: LinearGradient,
    ) {
        let corner_color = |px: f32, py: f32| {
            let t = gradient_t(px, py, gradient.x0, gradient.y0, gradient.x1, gradient.y1);
            lerp_color(gradient.start, gradient.end, t)
        };
        let colors = [
            corner_color(x, y),         // tl
            corner_color(x + w, y),     // tr
            corner_color(x, y + h),     // bl
            corner_color(x + w, y + h), // br
        ];
        let (tx, ty) = (x + self.translate_x, y + self.translate_y);
        let vertices =
            rect_vertices_colors(tx, ty, w, h, colors, self.width as f32, self.height as f32);
        self.submit_vertices(&vertices, false);
    }

    /// Same as [`Self::draw_gradient_rect`] but for a [`RadialGradient`] -
    /// see that type's own doc for why this needs a dedicated pipeline
    /// (`radial_pipeline`/[`RadialVertex`]) rather than the 4-corner-color
    /// trick `draw_gradient_rect` uses. Gradient coordinates are *not*
    /// offset by `translate_x`/`translate_y`, same documented cut as the
    /// linear case. `pub(super)` since `shapes::fill_rect` dispatches to
    /// this.
    pub(super) fn draw_radial_gradient_rect(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        gradient: RadialGradient,
    ) {
        let (tx, ty) = (x + self.translate_x, y + self.translate_y);
        let (vw, vh) = (self.width as f32, self.height as f32);
        let start = color_to_f32(gradient.start);
        let end = color_to_f32(gradient.end);
        let corner = |px: f32, py: f32| RadialVertex {
            position: point_to_ndc(px, py, vw, vh),
            local_pos: [px - gradient.cx, py - gradient.cy],
            radius: gradient.radius.max(0.001),
            start,
            end,
        };
        let tl = corner(tx, ty);
        let tr = corner(tx + w, ty);
        let bl = corner(tx, ty + h);
        let br = corner(tx + w, ty + h);
        let vertices = [tl, bl, tr, tr, bl, br];
        self.submit_radial_vertices(&vertices);
    }

    /// Same as `shapes::submit_vertices` but for a [`RadialGradient`] fill
    /// (`radial_pipeline`/[`RadialVertex`] instead of `fill_pipeline`/
    /// `Vertex`) - always alpha-blended source-over, never the
    /// `clear_pipeline` replace mode (no `clearRect` equivalent needs a
    /// gradient).
    fn submit_radial_vertices(&mut self, vertices: &[RadialVertex]) {
        let view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        use wgpu::util::DeviceExt;
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("canvas2d-radial-vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas2d-radial-draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.radial_pipeline);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
