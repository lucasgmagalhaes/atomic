//! Minimal WebGL-shaped API over `wgpu`: `create_shader`/`create_program`/
//! `draw_triangles`, backed by *real* GLSL compilation via naga's GLSL
//! frontend (not WGSL-in-disguise), rendered headlessly to an off-screen
//! texture and read back — same testability approach as `render::gpu`.
//!
//! Deliberately small subset of WebGL, plus one real limitation found
//! while building this: naga's GLSL frontend only parses **desktop** GLSL
//! (`#version 440/450/460`, `core` profile) — it rejects GLSL ES entirely
//! (`#version 300 es` fails with `InvalidVersion`/`InvalidProfile`). So
//! shader source here is desktop GLSL with explicit `layout(location=N)`
//! on `in`/`out`, not the GLSL ES actual WebGL requires. Getting real
//! GLSL ES support would need either a different frontend or preprocessing
//! `300 es` down to something naga accepts — not attempted here. Other
//! cuts:
//! - `gl.TRIANGLES` only, no other primitive topologies.
//! - No textures, uniforms, indices (`drawElements`), framebuffers,
//!   extensions, or WebGL2-only objects (VAOs, transform feedback, ...).
//! - No persistent bind state (no "current program"/VAO) — every draw
//!   call takes everything it needs, unlike real WebGL's stateful API.
use std::borrow::Cow;

use bytemuck::NoUninit;
use naga::ShaderStage as NagaStage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderType {
    Vertex,
    Fragment,
}

#[derive(Debug)]
pub struct Shader {
    module: naga::Module,
    stage: ShaderType,
}

#[derive(Debug)]
pub struct Program {
    pipeline: wgpu::RenderPipeline,
}

/// Mirrors `gl.vertexAttribPointer`'s parameters: `location` matches the
/// shader's `layout(location=N)`, `components` is how many `f32`s per
/// vertex for this attribute (1-4), `offset` is the byte offset into the
/// interleaved vertex buffer.
#[derive(Debug, Clone, Copy)]
pub struct VertexAttribute {
    pub location: u32,
    pub components: u32,
    pub offset: u64,
}

pub struct WebGl {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl WebGl {
    pub fn new() -> Self {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Self {
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
        WebGl { device, queue }
    }

    /// Compiles `source` (GLSL ES 3.00-style) for `stage`. Mirrors
    /// `gl.createShader` + `gl.shaderSource` + `gl.compileShader` collapsed
    /// into one call, since naga validates synchronously and there's no
    /// separate shader-object handle to attach source to later.
    pub fn create_shader(&self, stage: ShaderType, source: &str) -> Result<Shader, String> {
        let naga_stage = match stage {
            ShaderType::Vertex => NagaStage::Vertex,
            ShaderType::Fragment => NagaStage::Fragment,
        };
        let options = naga::front::glsl::Options::from(naga_stage);
        let module = naga::front::glsl::Frontend::default()
            .parse(&options, source)
            .map_err(|e| e.to_string())?;
        Ok(Shader { module, stage })
    }

    /// Mirrors `gl.createProgram` + `attachShader` ×2 + `linkProgram`:
    /// builds a render pipeline from a compiled vertex and fragment
    /// shader plus the interleaved vertex buffer's attribute layout.
    pub fn create_program(
        &self,
        vertex: &Shader,
        fragment: &Shader,
        attributes: &[VertexAttribute],
        stride: u64,
    ) -> Result<Program, String> {
        if vertex.stage != ShaderType::Vertex {
            return Err("vertex shader must be compiled with ShaderType::Vertex".into());
        }
        if fragment.stage != ShaderType::Fragment {
            return Err("fragment shader must be compiled with ShaderType::Fragment".into());
        }

        let vs = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("webgl-vertex"),
            source: wgpu::ShaderSource::Naga(Cow::Owned(vertex.module.clone())),
        });
        let fs = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("webgl-fragment"),
            source: wgpu::ShaderSource::Naga(Cow::Owned(fragment.module.clone())),
        });

        let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("webgl-program-layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });

        let vertex_attrs: Vec<wgpu::VertexAttribute> = attributes
            .iter()
            .map(|a| {
                let format = match a.components {
                    1 => wgpu::VertexFormat::Float32,
                    2 => wgpu::VertexFormat::Float32x2,
                    3 => wgpu::VertexFormat::Float32x3,
                    4 => wgpu::VertexFormat::Float32x4,
                    other => panic!("vertexAttribPointer: unsupported component count {other} (must be 1-4)"),
                };
                wgpu::VertexAttribute {
                    offset: a.offset,
                    shader_location: a.location,
                    format,
                }
            })
            .collect();

        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: stride,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &vertex_attrs,
        };

        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("webgl-program"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &vs,
                entry_point: "main",
                buffers: &[vertex_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &fs,
                entry_point: "main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            // TriangleList by default - matches gl.TRIANGLES, the only
            // primitive topology this crate supports.
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Ok(Program { pipeline })
    }

    /// Mirrors `gl.clearColor` + `gl.clear(gl.COLOR_BUFFER_BIT)` +
    /// `gl.bindBuffer`/`gl.bufferData` + `gl.drawArrays(gl.TRIANGLES, 0,
    /// vertex_count)`, collapsed into one call since this crate has no
    /// persistent bound state. Renders headlessly and returns
    /// tightly-packed RGBA8 pixels, row-major top-to-bottom - same
    /// readback convention as `render::GpuRenderer::render_to_rgba`.
    pub fn draw_triangles<V: NoUninit>(
        &self,
        program: &Program,
        vertex_data: &[V],
        vertex_count: u32,
        clear_color: [f64; 4],
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("webgl-render-target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        use wgpu::util::DeviceExt;
        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("webgl-vertex-buffer"),
            contents: bytemuck::cast_slice(vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("webgl-draw"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0],
                            g: clear_color[1],
                            b: clear_color[2],
                            a: clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if vertex_count > 0 {
                pass.set_pipeline(&program.pipeline);
                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                pass.draw(0..vertex_count, 0..1);
            }
        }

        // wgpu requires each buffer-copy row to be a multiple of
        // COPY_BYTES_PER_ROW_ALIGNMENT (256) - pad, then strip on readback.
        let unpadded_bytes_per_row = width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("webgl-readback"),
            size: (padded_bytes_per_row * height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &output_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
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
        let mut pixels = Vec::with_capacity((unpadded_bytes_per_row * height) as usize);
        for row in 0..height as usize {
            let start = row * padded_bytes_per_row as usize;
            let end = start + unpadded_bytes_per_row as usize;
            pixels.extend_from_slice(&padded[start..end]);
        }
        drop(padded);
        output_buffer.unmap();

        pixels
    }
}

impl Default for WebGl {
    fn default() -> Self {
        Self::new()
    }
}
