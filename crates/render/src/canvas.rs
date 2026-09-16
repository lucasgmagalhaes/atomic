//! Canvas 2D: `fillRect`/`clearRect` over `wgpu`, matching the imperative,
//! stateful shape of the real API — unlike `gpu::GpuRenderer` (one-shot:
//! whole display list in, pixels out), `Canvas2D` owns a persistent
//! texture that accumulates draws across calls, same as a real `<canvas>`
//! backing bitmap. Starts fully transparent, like the real spec.
//!
//! Scoped to solid-color/2-stop-linear-gradient rectangles:
//! `fillStyle`/`fillRect`/`clearRect` plus `strokeStyle`/`lineWidth`/
//! `strokeRect`, `save`/`restore`, `translate`, a `fillStyle` gradient via
//! `createLinearGradient` (see [`LinearGradient`]'s own doc for its exact
//! scope cuts), and a convex-only filled path
//! (`beginPath`/`moveTo`/`lineTo`/`closePath`/`fill` - see [`Canvas2D::fill`]'s
//! own doc for why only convex polygons render correctly) and one-line
//! `fillText` (see [`Canvas2D::fill_text`]'s own doc for its scope cuts:
//! fixed font/size, no `ctx.font`, no wrapping/metrics). No stroking a
//! path (only `strokeRect`'s rectangle-outline shortcut), no curves
//! (`arc`/`bezierCurveTo`/`quadraticCurveTo`), no drawImage sources
//! beyond another `<canvas>`, no radial/conic gradients or patterns, no
//! `scale`/`rotate`/general transform matrix (just plain translation), no
//! compositing modes beyond `fillRect`'s source-over and
//! `clearRect`'s hard replace-with-transparent.
use bytemuck::{Pod, Zeroable};

use layout_engine::{Color, FontFamily, GenericFontFamily};

use crate::display_list::ClippedGlyph;

/// `ctx.font`'s stand-in until that property is wired up - every
/// `fillText` call uses this fixed size and a generic sans-serif family,
/// see [`Canvas2D::fill_text`]'s own doc.
const DEFAULT_FONT_SIZE: f32 = 16.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

const SHADER_SRC: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(position, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

fn rect_vertices(x: f32, y: f32, w: f32, h: f32, color: [f32; 4], vw: f32, vh: f32) -> [Vertex; 6] {
    rect_vertices_colors(x, y, w, h, [color, color, color, color], vw, vh)
}

/// Like [`rect_vertices`] but with an independent color per corner
/// (`[tl, tr, bl, br]`) - what a gradient fill needs. The GPU's own
/// vertex-color interpolation across the two triangles does the rest, the
/// same zero-new-shader trick `render::gpu::shader::rect_to_vertices`
/// already uses for CSS `linear-gradient` backgrounds.
fn rect_vertices_colors(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    colors: [[f32; 4]; 4],
    vw: f32,
    vh: f32,
) -> [Vertex; 6] {
    let to_ndc_x = |px: f32| (px / vw) * 2.0 - 1.0;
    let to_ndc_y = |px: f32| 1.0 - (px / vh) * 2.0;
    let x0 = to_ndc_x(x);
    let x1 = to_ndc_x(x + w);
    let y0 = to_ndc_y(y);
    let y1 = to_ndc_y(y + h);
    let [tl_c, tr_c, bl_c, br_c] = colors;
    let tl = Vertex {
        position: [x0, y0],
        color: tl_c,
    };
    let tr = Vertex {
        position: [x1, y0],
        color: tr_c,
    };
    let bl = Vertex {
        position: [x0, y1],
        color: bl_c,
    };
    let br = Vertex {
        position: [x1, y1],
        color: br_c,
    };
    [tl, bl, tr, tr, bl, br]
}

/// `from`/`to` at blend position `t` (clamped to `[0, 1]`) - plain
/// per-channel linear interpolation, straight (non-premultiplied) alpha,
/// same shape as `render::gpu::shader`'s own private `lerp_color` (not
/// reused directly - that one is private to its own module).
fn lerp_color(from: Color, to: Color, t: f32) -> [f32; 4] {
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
fn gradient_t(px: f32, py: f32, x0: f32, y0: f32, x1: f32, y1: f32) -> f32 {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return 0.0;
    }
    (((px - x0) * dx + (py - y0) * dy) / len_sq).clamp(0.0, 1.0)
}

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

/// Pixel space (origin top-left, y-down) -> clip space (origin center,
/// y-up), the same conversion [`rect_vertices_colors`] does inline for
/// each rect corner - factored out for [`Canvas2D::fill`]'s path
/// triangulation, which has no fixed 4-corner shape to special-case.
fn point_to_ndc(x: f32, y: f32, vw: f32, vh: f32) -> [f32; 2] {
    [(x / vw) * 2.0 - 1.0, 1.0 - (y / vh) * 2.0]
}

fn color_to_f32(c: Color) -> [f32; 4] {
    [
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    ]
}

pub struct Canvas2D {
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture: wgpu::Texture,
    /// `source-over`-ish: blends onto whatever is already there. Used by
    /// `fill_rect`.
    fill_pipeline: wgpu::RenderPipeline,
    /// Hard replace, ignoring destination alpha entirely - what
    /// `clear_rect` needs (alpha-blending a transparent color onto an
    /// opaque pixel would leave it untouched, which is wrong for clear).
    clear_pipeline: wgpu::RenderPipeline,
    width: u32,
    height: u32,
    fill_style: Color,
    stroke_style: Color,
    line_width: f32,
    /// `ctx.fillStyle = gradient`'s backing, when set - overrides
    /// `fill_style` for `fill_rect` only (`clearRect`/`strokeRect` stay
    /// solid-color; real spec lets any of them use a gradient, this crate
    /// only wires the one most common case). `set_fill_style` clears this
    /// back to `None`, matching real spec's "assigning `fillStyle` replaces
    /// whatever was there before, solid or gradient".
    fill_gradient: Option<LinearGradient>,
    /// `ctx.translate(x, y)`'s accumulated offset - the one transform this
    /// crate supports (no scale/rotate/general matrix - see
    /// [`Self::translate`]'s own doc). Added to every `fillRect`/
    /// `clearRect`/`strokeRect` coordinate before painting.
    translate_x: f32,
    translate_y: f32,
    /// `ctx.save()`/`ctx.restore()`'s backing stack - scoped to just the
    /// drawing-state fields this crate actually has (`fillStyle`/
    /// `strokeStyle`/`lineWidth`/translate offset), not real spec's full
    /// state (no clip region, general transform matrix, or compositing/
    /// font state exists here to save).
    state_stack: Vec<CanvasState>,
    /// `beginPath`/`moveTo`/`lineTo`'s backing point list - not part of
    /// `save`/`restore`'s state stack, matching real spec (the current
    /// path is its own separate piece of state, untouched by save/
    /// restore). `moveTo` and `lineTo` are functionally identical here -
    /// both just append a point; real spec's subpath-boundary distinction
    /// isn't modeled, since this crate only ever fills one polygon per
    /// `beginPath`, not multiple subpaths.
    path_points: Vec<(f32, f32)>,
}

#[derive(Clone, Copy)]
struct CanvasState {
    fill_style: Color,
    fill_gradient: Option<LinearGradient>,
    stroke_style: Color,
    line_width: f32,
    translate_x: f32,
    translate_y: f32,
}

impl Canvas2D {
    pub fn new(width: u32, height: u32) -> Self {
        pollster::block_on(Self::new_async(width, height))
    }

    async fn new_async(width: u32, height: u32) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect(
                "no wgpu adapter available - this needs a GPU (or software fallback) on the host",
            );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("failed to get wgpu device");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas2d-quad"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("canvas2d-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };
        let make_pipeline = |blend: wgpu::BlendState, label: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[vertex_layout.clone()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
            })
        };
        let fill_pipeline = make_pipeline(wgpu::BlendState::ALPHA_BLENDING, "canvas2d-fill");
        let clear_pipeline = make_pipeline(wgpu::BlendState::REPLACE, "canvas2d-clear");

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("canvas2d-backing"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let mut canvas = Canvas2D {
            device,
            queue,
            texture,
            fill_pipeline,
            clear_pipeline,
            width,
            height,
            fill_style: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            stroke_style: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            line_width: 1.0,
            fill_gradient: None,
            translate_x: 0.0,
            translate_y: 0.0,
            state_stack: Vec::new(),
            path_points: Vec::new(),
        };
        // The real spec starts a canvas fully transparent, not undefined.
        canvas.clear_rect(0.0, 0.0, width as f32, height as f32);
        canvas
    }

    pub fn set_fill_style(&mut self, color: Color) {
        self.fill_style = color;
        self.fill_gradient = None;
    }

    /// `ctx.fillStyle`'s getter side — a JS binding (`js-runtime`'s
    /// `canvas_bindings`) formats this back to a `#rrggbb` hex string.
    pub fn fill_style(&self) -> Color {
        self.fill_style
    }

    /// `ctx.fillStyle = gradient` — sets a [`LinearGradient`] as the fill
    /// paint for subsequent `fill_rect` calls, without touching the plain
    /// `fill_style` color underneath (so a later `set_fill_style` still has
    /// something sane to fall back to, and `fill_style()`'s own getter
    /// keeps returning that last solid color - see its own doc for why
    /// this crate doesn't round-trip the actual gradient object back out).
    pub fn set_fill_gradient(&mut self, gradient: LinearGradient) {
        self.fill_gradient = Some(gradient);
    }

    /// `Some` when the current fill paint is a gradient, not a plain
    /// `fill_style` color.
    pub fn fill_gradient(&self) -> Option<LinearGradient> {
        self.fill_gradient
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
    /// [`Self::state_stack`].
    pub fn save(&mut self) {
        self.state_stack.push(CanvasState {
            fill_style: self.fill_style,
            fill_gradient: self.fill_gradient,
            stroke_style: self.stroke_style,
            line_width: self.line_width,
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

    /// `ctx.beginPath()` — discards any points accumulated since the last
    /// call.
    pub fn begin_path(&mut self) {
        self.path_points.clear();
    }

    /// `ctx.moveTo(x, y)` — see [`Self::path_points`]'s own doc for why
    /// this behaves identically to [`Self::line_to`] in this crate.
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.path_points.push((x, y));
    }

    /// `ctx.lineTo(x, y)`.
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.path_points.push((x, y));
    }

    /// `ctx.closePath()` — a no-op: [`Self::fill`]'s fan triangulation
    /// already treats `path_points` as an implicitly closed polygon (its
    /// last point connects back to the first), so there's no separate
    /// "closing segment" state to track.
    pub fn close_path(&mut self) {}

    /// `ctx.fill()` — fills the current path with the plain `fill_style`
    /// color via fan triangulation from `path_points[0]` (`(p0, p[i],
    /// p[i+1])` for each `i` in `1..len-1`). **Exact only for convex
    /// polygons** - a concave path will paint the wrong region (some
    /// fan triangles fall outside the intended shape), since this crate
    /// has no general polygon triangulator (ear-clipping or similar). No
    /// gradient fill for paths (unlike `fillRect` - a documented, narrower
    /// cut to avoid duplicating the gradient-projection logic for
    /// arbitrary triangle geometry). Curves (`arc`/`bezierCurveTo`/
    /// `quadraticCurveTo`) aren't supported - only straight `lineTo`
    /// segments. Does nothing on fewer than 3 points.
    pub fn fill(&mut self) {
        if self.path_points.len() < 3 {
            return;
        }
        let (tx, ty) = (self.translate_x, self.translate_y);
        let (vw, vh) = (self.width as f32, self.height as f32);
        let color = color_to_f32(self.fill_style);
        let p0 = self.path_points[0];
        let mut vertices = Vec::with_capacity((self.path_points.len() - 2) * 3);
        for pair in self.path_points[1..].windows(2) {
            let (p1, p2) = (pair[0], pair[1]);
            for p in [p0, p1, p2] {
                vertices.push(Vertex {
                    position: point_to_ndc(p.0 + tx, p.1 + ty, vw, vh),
                    color,
                });
            }
        }
        self.submit_vertices(&vertices, false);
    }

    /// `ctx.fillText(text, x, y)` — real shaping/rasterization via
    /// `layout_engine::layout_text` (`cosmic-text`+`swash`, the same real
    /// pipeline page text uses) and `render::text::composite_glyphs` (the
    /// same CPU alpha-blend compositor page text uses), not a stub. Built
    /// as a full-canvas `get_image_data`/`composite_glyphs`/
    /// `put_image_data` round trip - correctness over throughput, same
    /// tradeoff `render::text`'s own module doc already makes, and no new
    /// GPU pipeline needed.
    ///
    /// Scope cuts: no `ctx.font` yet - every call uses a fixed
    /// [`DEFAULT_FONT_SIZE`] and a generic sans-serif family (real spec's
    /// own default is `"10px sans-serif"`; this crate's fixed size is
    /// larger for legibility, not a spec match). No text wrapping (`x`, `y`
    /// only - the `maxWidth` 4th argument isn't accepted, matching
    /// `layout_text`'s own `max_width: None` = single unbounded line). `y`
    /// behaves like real spec's `textBaseline = "top"` (measured from the
    /// text's own top, not the default `"alphabetic"` baseline) - this
    /// reuses `layout_text`'s glyphs exactly as `layout_engine`'s own
    /// page-text pipeline positions them (box-top-relative), with no
    /// separate baseline-offset math added on top. No `strokeText`,
    /// `measureText`, or gradient fill (`fillStyle`'s plain color only).
    pub fn fill_text(&mut self, text: &str, x: f32, y: f32) {
        if text.is_empty() {
            return;
        }
        let layout = layout_engine::layout_text(
            text,
            DEFAULT_FONT_SIZE,
            None,
            self.fill_style,
            FontFamily::generic(GenericFontFamily::SansSerif),
        );
        if layout.glyphs.is_empty() {
            return;
        }
        let (tx, ty) = (
            (x + self.translate_x).round() as i32,
            (y + self.translate_y).round() as i32,
        );
        let glyphs: Vec<ClippedGlyph> = layout
            .glyphs
            .iter()
            .map(|g| {
                let mut positioned = *g;
                positioned.x += tx;
                positioned.y += ty;
                ClippedGlyph {
                    glyph: positioned,
                    clip: None,
                    opacity: 1.0,
                    fixed: false,
                    sticky: None,
                }
            })
            .collect();

        let mut pixels = self.get_image_data();
        crate::text::composite_glyphs(&mut pixels, self.width, self.height, &glyphs);
        self.put_image_data(0, 0, self.width, self.height, &pixels);
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

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

    /// Same as [`Self::draw_rect`] but with a [`LinearGradient`] fill
    /// instead of a solid color - computes each corner's exact color by
    /// projecting it onto the gradient line (see [`gradient_t`]), then lets
    /// the GPU's own vertex-color interpolation fill in the interior.
    /// Gradient coordinates are *not* offset by `translate_x`/`translate_y`
    /// (see [`LinearGradient`]'s own doc on this scope cut) - only the
    /// rect's own position is.
    fn draw_gradient_rect(&mut self, x: f32, y: f32, w: f32, h: f32, gradient: LinearGradient) {
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

    /// Submits an arbitrary triangle-list vertex buffer (`vertices.len()`
    /// must be a multiple of 3) - rects always pass exactly 6 (two
    /// triangles), a filled path (see [`Self::fill`]) passes
    /// `(point_count - 2) * 3` from its fan triangulation.
    fn submit_vertices(&mut self, vertices: &[Vertex], replace: bool) {
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
    /// gradient (see [`Self::set_fill_gradient`]) if one is set, else the
    /// plain `fill_style` color.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        match self.fill_gradient {
            Some(gradient) => self.draw_gradient_rect(x, y, w, h, gradient),
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

    /// `ctx.getImageData(0, 0, width, height).data` — tightly-packed RGBA8
    /// pixels, row-major top-to-bottom.
    pub fn get_image_data(&self) -> Vec<u8> {
        let unpadded_bytes_per_row = self.width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas2d-readback"),
            size: (padded_bytes_per_row * self.height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv().unwrap().expect("failed to map readback buffer");

        let padded = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((unpadded_bytes_per_row * self.height) as usize);
        for row in 0..self.height as usize {
            let start = row * padded_bytes_per_row as usize;
            let end = start + unpadded_bytes_per_row as usize;
            pixels.extend_from_slice(&padded[start..end]);
        }
        drop(padded);
        output_buffer.unmap();
        pixels
    }

    /// `ctx.putImageData(imageData, x, y)` — writes `rgba` (tightly packed
    /// RGBA8, row-major top-to-bottom, `w * h * 4` bytes, same layout
    /// [`Canvas2D::get_image_data`] returns) directly into the backing
    /// texture at `(x, y)`, clipped to stay inside `[0, width) x [0,
    /// height)` (a region that doesn't fully fit is silently cropped
    /// rather than erroring, matching this crate's general "best-effort,
    /// no separate error path" paint convention). A raw memory write, not
    /// a blended draw — like `clear_rect`'s `REPLACE` pipeline, existing
    /// pixels underneath are fully overwritten, not blended with.
    pub fn put_image_data(&mut self, x: i32, y: i32, w: u32, h: u32, rgba: &[u8]) {
        let src_row_bytes = (w * 4) as usize;
        debug_assert!(rgba.len() >= src_row_bytes * h as usize);

        let dst_x = x.max(0) as u32;
        let dst_y = y.max(0) as u32;
        if dst_x >= self.width || dst_y >= self.height {
            return;
        }
        // How many of `rgba`'s own rows/columns a negative `x`/`y` already
        // skips (0 when `x`/`y` is `>= 0`) - clamping `copy_w`/`copy_h`
        // needs both this *and* how much room is left in the destination
        // canvas, or a negative origin alone (destination clip: none) would
        // read past `rgba`'s own end - see this test suite's own
        // `put_image_data_clips_a_negative_origin` for the regression this
        // closes.
        let src_x_skip = (dst_x as i32 - x) as u32;
        let src_y_skip = (dst_y as i32 - y) as u32;
        let copy_w = w
            .saturating_sub(src_x_skip)
            .min(self.width.saturating_sub(dst_x));
        let copy_h = h
            .saturating_sub(src_y_skip)
            .min(self.height.saturating_sub(dst_y));
        if copy_w == 0 || copy_h == 0 {
            return;
        }

        // A cropped copy (x/y negative, or the region spills past the
        // canvas edge) needs its own tightly-packed buffer - `write_texture`
        // has no "skip these source bytes" concept, only a uniform
        // `bytes_per_row` stride over the *destination* rectangle's own
        // width.
        let src_x_offset = src_x_skip as usize * 4;
        let src_y_offset = src_y_skip as usize;
        let mut cropped = Vec::with_capacity((copy_w * copy_h * 4) as usize);
        for row in 0..copy_h as usize {
            let start = (src_y_offset + row) * src_row_bytes + src_x_offset;
            let end = start + (copy_w * 4) as usize;
            cropped.extend_from_slice(&rgba[start..end]);
        }

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: dst_x,
                    y: dst_y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &cropped,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(copy_w * 4),
                rows_per_image: Some(copy_h),
            },
            wgpu::Extent3d {
                width: copy_w,
                height: copy_h,
                depth_or_array_layers: 1,
            },
        );
    }
}
