//! `WebGl::create_shader`/`create_program` — split out from `context.rs`.

use std::borrow::Cow;

use naga::ShaderStage as NagaStage;

use super::construct::WebGl;
use super::glsl::rewrite_glsl_es_version;
use super::types::{Program, Shader, ShaderType, VertexAttribute};

impl WebGl {
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
        let rewritten = rewrite_glsl_es_version(source);
        let module = naga::front::glsl::Frontend::default()
            .parse(&options, &rewritten)
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

        let vs = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("webgl-vertex"),
                source: wgpu::ShaderSource::Naga(Cow::Owned(vertex.module.clone())),
            });
        let fs = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("webgl-fragment"),
                source: wgpu::ShaderSource::Naga(Cow::Owned(fragment.module.clone())),
            });

        let layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("webgl-program-layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        let vertex_attrs: Vec<wgpu::VertexAttribute> =
            attributes
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

        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
}
