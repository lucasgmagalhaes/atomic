//! `GpuRenderer::new`/`new_with_adapter`/`new_async` — split out from
//! `gpu.rs`.

use super::shader::{Vertex, SHADER_SRC};
use super::GpuRenderer;

impl GpuRenderer {
    pub fn new() -> Self {
        pollster::block_on(Self::new_async(None))
    }

    /// Same as [`new`](Self::new), but opens the `index`-th adapter from
    /// [`super::list_adapters`]'s own enumeration order instead of
    /// letting `wgpu::Instance::request_adapter`'s default heuristic pick
    /// one - the real primitive behind a GPU-selection setting. Panics
    /// (same convention as `new`'s own `expect`s - this crate has no
    /// recovery path for "no usable GPU" anywhere yet) if `index` is out
    /// of range or that specific adapter fails to open a device.
    pub fn new_with_adapter(index: usize) -> Self {
        pollster::block_on(Self::new_async(Some(index)))
    }

    pub(super) async fn new_async(adapter_index: Option<usize>) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let adapter = match adapter_index {
      Some(index) => {
        let mut adapters = instance.enumerate_adapters(wgpu::Backends::PRIMARY);
        if index >= adapters.len() {
          panic!(
            "adapter index {index} out of range - only {} adapter(s) enumerated",
            adapters.len()
          );
        }
        adapters.remove(index)
      }
      None => instance
        .request_adapter(&wgpu::RequestAdapterOptions {
          power_preference: wgpu::PowerPreference::default(),
          compatible_surface: None,
          force_fallback_adapter: false,
        })
        .await
        .expect("no wgpu adapter available - this needs a GPU (or software fallback) on the host"),
    };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("failed to get wgpu device");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("solid-color-quad"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("solid-color-quad-layout"),
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
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress
                        + std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress
                        + std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress
                        + std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("solid-color-quad-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vertex_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        GpuRenderer {
            device,
            queue,
            pipeline,
        }
    }
}
