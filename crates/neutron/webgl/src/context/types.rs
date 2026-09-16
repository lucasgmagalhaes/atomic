//! `ShaderType`/`Shader`/`Program`/`VertexAttribute` — split out from
//! `context.rs`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderType {
    Vertex,
    Fragment,
}

#[derive(Debug)]
pub struct Shader {
    pub(super) module: naga::Module,
    pub(super) stage: ShaderType,
}

#[derive(Debug)]
pub struct Program {
    pub(super) pipeline: wgpu::RenderPipeline,
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
