//! GPU vertex layouts, WGSL shader sources, and one-time pipeline/device
//! setup for `Canvas2D` — split out from `mod.rs` since this is pure
//! device-construction boilerplate (`build`), not per-draw-call drawing
//! logic. `Vertex`/`RadialVertex` and the two `rect_vertices*` builders
//! are used by `super::shapes`/`super::gradients`/`super::path`, hence
//! `pub(super)`.
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Vertex {
    pub(super) position: [f32; 2],
    pub(super) color: [f32; 4],
}

pub(super) const SHADER_SRC: &str = r#"
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

/// `corners` is `[tl, tr, bl, br]` in pixel space (already transform-
/// matrix-applied by the caller - see `Canvas2D::transform_point`) -
/// arbitrary quad, not necessarily axis-aligned once `scale`/`rotate` are
/// in play.
pub(super) fn rect_vertices(
    corners: [(f32, f32); 4],
    color: [f32; 4],
    vw: f32,
    vh: f32,
) -> [Vertex; 6] {
    rect_vertices_colors(corners, [color, color, color, color], vw, vh)
}

/// Like [`rect_vertices`] but with an independent color per corner
/// (`[tl, tr, bl, br]`) - what a gradient fill needs. The GPU's own
/// vertex-color interpolation across the two triangles does the rest, the
/// same zero-new-shader trick `render::gpu::shader::rect_to_vertices`
/// already uses for CSS `linear-gradient` backgrounds.
pub(super) fn rect_vertices_colors(
    corners: [(f32, f32); 4],
    colors: [[f32; 4]; 4],
    vw: f32,
    vh: f32,
) -> [Vertex; 6] {
    let to_ndc = |(px, py): (f32, f32)| [(px / vw) * 2.0 - 1.0, 1.0 - (py / vh) * 2.0];
    let [tl_p, tr_p, bl_p, br_p] = corners;
    let [tl_c, tr_c, bl_c, br_c] = colors;
    let tl = Vertex {
        position: to_ndc(tl_p),
        color: tl_c,
    };
    let tr = Vertex {
        position: to_ndc(tr_p),
        color: tr_c,
    };
    let bl = Vertex {
        position: to_ndc(bl_p),
        color: bl_c,
    };
    let br = Vertex {
        position: to_ndc(br_p),
        color: br_c,
    };
    [tl, bl, tr, tr, bl, br]
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct RadialVertex {
    pub(super) position: [f32; 2],
    /// This vertex's position relative to the gradient's own center, in
    /// pixel units (not NDC) - see [`super::gradients::RadialGradient`]'s
    /// own doc on why interpolating *this* (an affine function of
    /// position), rather than a pre-computed per-vertex color, is what
    /// makes this exact.
    pub(super) local_pos: [f32; 2],
    pub(super) radius: f32,
    pub(super) start: [f32; 4],
    pub(super) end: [f32; 4],
}

pub(super) const RADIAL_SHADER_SRC: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) radius: f32,
    @location(2) start: vec4<f32>,
    @location(3) end: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) radius: f32,
    @location(3) start: vec4<f32>,
    @location(4) end: vec4<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(position, 0.0, 1.0);
    out.local_pos = local_pos;
    out.radius = radius;
    out.start = start;
    out.end = end;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = clamp(length(in.local_pos) / in.radius, 0.0, 1.0);
    return mix(in.start, in.end, t);
}
"#;

/// Bundled GPU resources [`super::Canvas2D::new_async`] builds once at
/// construction - device/queue/backing texture plus the three render
/// pipelines (solid fill, hard-replace clear, radial gradient).
pub(super) struct GpuResources {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) texture: wgpu::Texture,
    pub(super) fill_pipeline: wgpu::RenderPipeline,
    pub(super) clear_pipeline: wgpu::RenderPipeline,
    pub(super) radial_pipeline: wgpu::RenderPipeline,
}

pub(super) async fn build(width: u32, height: u32) -> GpuResources {
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

    let radial_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("canvas2d-radial-gradient"),
        source: wgpu::ShaderSource::Wgsl(RADIAL_SHADER_SRC.into()),
    });
    let radial_vertex_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<RadialVertex>() as wgpu::BufferAddress,
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
                format: wgpu::VertexFormat::Float32x2,
            },
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                shader_location: 2,
                format: wgpu::VertexFormat::Float32,
            },
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 5]>() as wgpu::BufferAddress,
                shader_location: 3,
                format: wgpu::VertexFormat::Float32x4,
            },
            wgpu::VertexAttribute {
                offset: std::mem::size_of::<[f32; 9]>() as wgpu::BufferAddress,
                shader_location: 4,
                format: wgpu::VertexFormat::Float32x4,
            },
        ],
    };
    let radial_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("canvas2d-radial-gradient"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &radial_shader,
            entry_point: "vs_main",
            buffers: &[radial_vertex_layout],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &radial_shader,
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

    GpuResources {
        device,
        queue,
        texture,
        fill_pipeline,
        clear_pipeline,
        radial_pipeline,
    }
}
