//! Facade crate for the browser's compile/layout/paint/execute pipeline.
//! Owns the boundary between this pipeline and everything outside it
//! (`crates/atomic`, `profile`/`profile-worker`, `automation`): those depend
//! on `neutron` only, never on the sub-crates re-exported below directly.
//! See `README.md` for the pipeline shape and `spec/proposals/NEUTRON_ENCAPSULATION.md`
//! for why this crate exists.

pub use atoms;
pub use css;
pub use dom;
pub use html;
pub use image_decode as image;
pub use js_runtime as js;
pub use layout_engine as layout;
pub use quickjs_sys;
pub use render as paint;
pub use webgl as gl;
pub use workers;
