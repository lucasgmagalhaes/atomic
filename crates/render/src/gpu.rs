//! GPU compositor: rasterizes a display list (`Vec<Rect>`) to an
//! off-screen RGBA texture via `wgpu`. No window/surface — renders to a
//! plain `Texture` and reads the pixels back, which is what makes this
//! testable without a display. Solid-color quads only, matching
//! `display_list`'s scope: one draw call, two triangles per rect, no
//! blending beyond straight alpha-over-opaque-black (the clear color).
use bytemuck::{Pod, Zeroable};

use crate::display_list::Rect;

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

fn rect_to_vertices(rect: &Rect, viewport_width: f32, viewport_height: f32) -> [Vertex; 6] {
  // Pixel space (origin top-left, y-down) -> clip space (origin center,
  // y-up), which is why the y term is negated.
  let to_ndc_x = |px: f32| (px / viewport_width) * 2.0 - 1.0;
  let to_ndc_y = |px: f32| 1.0 - (px / viewport_height) * 2.0;

  let x0 = to_ndc_x(rect.x);
  let x1 = to_ndc_x(rect.x + rect.width);
  let y0 = to_ndc_y(rect.y);
  let y1 = to_ndc_y(rect.y + rect.height);

  let color = [
    rect.color.r as f32 / 255.0,
    rect.color.g as f32 / 255.0,
    rect.color.b as f32 / 255.0,
    rect.color.a as f32 / 255.0,
  ];

  let tl = Vertex {
    position: [x0, y0],
    color,
  };
  let tr = Vertex {
    position: [x1, y0],
    color,
  };
  let bl = Vertex {
    position: [x0, y1],
    color,
  };
  let br = Vertex {
    position: [x1, y1],
    color,
  };

  [tl, bl, tr, tr, bl, br]
}

/// One real GPU adapter's identity, for a caller (Settings > Performance
/// > GPU) to show actual hardware options instead of fake ones. Mirrors
/// the fields of `wgpu::AdapterInfo` this crate's callers actually need
/// to display/distinguish adapters by, without leaking the `wgpu` type
/// itself into every caller's dependency surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterInfo {
  pub name: String,
  /// e.g. "Vulkan", "Dx12", "Metal", "Gl" — `wgpu::Backend`'s own
  /// `Display` impl, not reinvented here.
  pub backend: String,
  pub device_type: AdapterDeviceType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterDeviceType {
  DiscreteGpu,
  IntegratedGpu,
  VirtualGpu,
  Cpu,
  Other,
}

impl From<wgpu::DeviceType> for AdapterDeviceType {
  fn from(dt: wgpu::DeviceType) -> Self {
    match dt {
      wgpu::DeviceType::DiscreteGpu => AdapterDeviceType::DiscreteGpu,
      wgpu::DeviceType::IntegratedGpu => AdapterDeviceType::IntegratedGpu,
      wgpu::DeviceType::VirtualGpu => AdapterDeviceType::VirtualGpu,
      wgpu::DeviceType::Cpu => AdapterDeviceType::Cpu,
      wgpu::DeviceType::Other => AdapterDeviceType::Other,
    }
  }
}

/// Real enumeration of every GPU adapter `wgpu` can see on this machine
/// (`wgpu::Instance::enumerate_adapters` + each `Adapter::get_info()`) -
/// not a fixed/fake list. Order matches what [`GpuRenderer::
/// new_with_adapter`]'s `index` indexes into. Synchronous - enumeration
/// itself doesn't need `request_adapter`'s async device negotiation.
pub fn list_adapters() -> Vec<AdapterInfo> {
  let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
    backends: wgpu::Backends::PRIMARY,
    ..Default::default()
  });
  instance
    .enumerate_adapters(wgpu::Backends::PRIMARY)
    .into_iter()
    .map(|adapter| {
      let info = adapter.get_info();
      AdapterInfo {
        name: info.name,
        backend: info.backend.to_string(),
        device_type: info.device_type.into(),
      }
    })
    .collect()
}

pub struct GpuRenderer {
  device: wgpu::Device,
  queue: wgpu::Queue,
  pipeline: wgpu::RenderPipeline,
}

impl GpuRenderer {
  pub fn new() -> Self {
    pollster::block_on(Self::new_async(None))
  }

  /// Same as [`new`](Self::new), but opens the `index`-th adapter from
  /// [`list_adapters`]'s own enumeration order instead of letting
  /// `wgpu::Instance::request_adapter`'s default heuristic pick one -
  /// the real primitive behind a GPU-selection setting. Panics (same
  /// convention as `new`'s own `expect`s - this crate has no recovery
  /// path for "no usable GPU" anywhere yet) if `index` is out of range
  /// or that specific adapter fails to open a device.
  pub fn new_with_adapter(index: usize) -> Self {
    pollster::block_on(Self::new_async(Some(index)))
  }

  async fn new_async(adapter_index: Option<usize>) -> Self {
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

  /// Renders `rects` onto a `width` × `height` canvas cleared to
  /// `clear_color` (straight RGBA, `0.0..=1.0` per channel) and returns
  /// tightly-packed RGBA8 pixel bytes, row-major, top-to-bottom.
  pub fn render_to_rgba(
    &self,
    rects: &[Rect],
    width: u32,
    height: u32,
    clear_color: [f64; 4],
  ) -> Vec<u8> {
    let texture = self.device.create_texture(&wgpu::TextureDescriptor {
      label: Some("render-target"),
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

    let vertices: Vec<Vertex> = rects
      .iter()
      .flat_map(|r| rect_to_vertices(r, width as f32, height as f32))
      .collect();

    let vertex_buffer = if vertices.is_empty() {
      None
    } else {
      use wgpu::util::DeviceExt;
      Some(
        self
          .device
          .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad-vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
          }),
      )
    };

    let mut encoder = self
      .device
      .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    {
      let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("paint"),
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

      if let Some(vertex_buffer) = &vertex_buffer {
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
      }
    }

    // wgpu requires each row of a buffer copy target to be a multiple
    // of COPY_BYTES_PER_ROW_ALIGNMENT (256) - width*4 rarely is, so we
    // copy into a padded buffer and strip the padding back out below.
    let unpadded_bytes_per_row = width * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;

    let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("readback"),
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

impl Default for GpuRenderer {
  fn default() -> Self {
    Self::new()
  }
}
