//! Canvas 2D: `fillRect`/`clearRect` over `wgpu`, matching the imperative,
//! stateful shape of the real API — unlike `gpu::GpuRenderer` (one-shot:
//! whole display list in, pixels out), `Canvas2D` owns a persistent
//! texture that accumulates draws across calls, same as a real `<canvas>`
//! backing bitmap. Starts fully transparent, like the real spec.
//!
//! Scoped to solid-color rectangles: `fillStyle` + `fillRect`/`clearRect`
//! only. No paths (`beginPath`/`lineTo`/`arc`/...), no strokes, no text,
//! no images/`drawImage`, no gradients/patterns, no transforms, no
//! compositing modes beyond `fillRect`'s source-over and `clearRect`'s
//! hard replace-with-transparent.
use bytemuck::{Pod, Zeroable};

use layout_engine::Color;

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
    let to_ndc_x = |px: f32| (px / vw) * 2.0 - 1.0;
    let to_ndc_y = |px: f32| 1.0 - (px / vh) * 2.0;
    let x0 = to_ndc_x(x);
    let x1 = to_ndc_x(x + w);
    let y0 = to_ndc_y(y);
    let y1 = to_ndc_y(y + h);
    let tl = Vertex { position: [x0, y0], color };
    let tr = Vertex { position: [x1, y0], color };
    let bl = Vertex { position: [x0, y1], color };
    let br = Vertex { position: [x1, y1], color };
    [tl, bl, tr, tr, bl, br]
}

fn color_to_f32(c: Color) -> [f32; 4] {
    [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0, c.a as f32 / 255.0]
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
            .expect("no wgpu adapter available - this needs a GPU (or software fallback) on the host");
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
                wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
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
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
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
            fill_style: Color { r: 0, g: 0, b: 0, a: 255 },
        };
        // The real spec starts a canvas fully transparent, not undefined.
        canvas.clear_rect(0.0, 0.0, width as f32, height as f32);
        canvas
    }

    pub fn set_fill_style(&mut self, color: Color) {
        self.fill_style = color;
    }

    fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color, replace: bool) {
        let view = self.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let vertices = rect_vertices(x, y, w, h, color_to_f32(color), self.width as f32, self.height as f32);

        use wgpu::util::DeviceExt;
        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("canvas2d-rect-vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("canvas2d-draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    // Load, not Clear: this canvas accumulates draws across
                    // calls, unlike GpuRenderer's one-shot render_to_rgba.
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(if replace { &self.clear_pipeline } else { &self.fill_pipeline });
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..6, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// `ctx.fillRect(x, y, w, h)` using the current `fillStyle`.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.draw_rect(x, y, w, h, self.fill_style, false);
    }

    /// `ctx.clearRect(x, y, w, h)` — always transparent, regardless of
    /// `fillStyle`, and always a hard replace (see `clear_pipeline`).
    pub fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.draw_rect(x, y, w, h, Color::TRANSPARENT, true);
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

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
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
            wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
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
}
