//! Minimal WebGL-shaped API over `wgpu`: `create_shader`/`create_program`/
//! `draw_triangles`, backed by *real* GLSL compilation via naga's GLSL
//! frontend (not WGSL-in-disguise), rendered headlessly to an off-screen
//! texture and read back — same testability approach as `render::gpu`.
//!
//! `create_shader` now accepts real GLSL ES 3.00 source (`#version 300
//! es`), the actual WebGL2 shading language — not just desktop GLSL. The
//! fix is narrower than it sounds: naga's GLSL frontend hard-codes its
//! accepted `#version` line to `440`/`450`/`460` + `core` (read straight
//! out of its parser source, not inferred from the error message), but
//! everything *after* that line — precision qualifiers, `in`/`out`,
//! `layout(location=N)`, the whole expression/statement grammar — is
//! already shared between GLSL ES 3.00 and desktop core GLSL 4.50 in
//! naga's parser (it accepts `precision highp float;` unconditionally,
//! regardless of profile). So [`rewrite_glsl_es_version`] rewrites just
//! that one directive line (`#version 300/310/320 es` → `#version 450
//! core`) before handing the source to naga — real ES source in, no
//! changes required from the caller. What this doesn't cover: GLSL ES
//! 1.00 (WebGL1's `attribute`/`varying` shading language, not just a
//! different version number — different enough grammar that a version-line
//! rewrite alone wouldn't work), and any ES 3.x construct naga's core
//! grammar genuinely doesn't implement (naga's GLSL frontend is itself a
//! subset of desktop GLSL, not a complete implementation).
//!
//! Other cuts, unrelated to the version issue:
//! - `gl.TRIANGLES` only, no other primitive topologies.
//! - No textures, uniforms, indices (`drawElements`), framebuffers,
//!   extensions, or WebGL2-only objects (VAOs, transform feedback, ...).
//! - No persistent bind state (no "current program"/VAO) — every draw
//!   call takes everything it needs, unlike real WebGL's stateful API.
//!
//! Split into `types.rs` (`ShaderType`/`Shader`/`Program`/
//! `VertexAttribute`), `glsl.rs` (`rewrite_glsl_es_version`),
//! `construct.rs` (`WebGl`'s struct def + `new`/`new_async`), `shader.rs`
//! (`create_shader`/`create_program`), and `draw.rs` (`draw_triangles`).

mod construct;
mod draw;
mod glsl;
mod shader;
mod types;

pub use construct::WebGl;
pub use types::{Program, Shader, ShaderType, VertexAttribute};
