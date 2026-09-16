//! GPU compositor: rasterizes a display list (`Vec<Rect>`) to an
//! off-screen RGBA texture via `wgpu`. No window/surface — renders to a
//! plain `Texture` and reads the pixels back, which is what makes this
//! testable without a display. Solid-color quads only, matching
//! `display_list`'s scope: one draw call, two triangles per rect, no
//! blending beyond straight alpha-over-opaque-black (the clear color) —
//! plus real, anti-aliased `border-radius` now (see `sd_rounded_box` in
//! `shader::SHADER_SRC`): every vertex carries the rect's own half-size
//! and radius alongside its position-within-the-rect, and the fragment
//! shader computes a signed distance to the rounded-box edge per pixel,
//! `smoothstep`-blending alpha across a 1px band at the boundary. A
//! `radius <= 0.0` rect takes an exact early-return in `fs_main` (`return
//! in.color`), so every existing radius-less call site paints byte-for-
//! byte identical pixels to before this landed.
//!
//! Split into `adapter.rs` (`AdapterInfo`/`AdapterDeviceType`/
//! `list_adapters`), `shader.rs` (the WGSL shader source, `Vertex`, and
//! `rect_to_vertices`), `construct.rs` (`GpuRenderer::new`/
//! `new_with_adapter`/`new_async`), and `render.rs`
//! (`GpuRenderer::render_to_rgba`) — this file keeps the `GpuRenderer`
//! struct definition and its `Default` impl.

mod adapter;
mod construct;
mod render;
mod shader;

pub use adapter::{list_adapters, AdapterDeviceType, AdapterInfo};

pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
}

impl Default for GpuRenderer {
    fn default() -> Self {
        Self::new()
    }
}
