//! Canvas 2D: `fillRect`/`clearRect` over `wgpu`, matching the imperative,
//! stateful shape of the real API — unlike `gpu::GpuRenderer` (one-shot:
//! whole display list in, pixels out), `Canvas2D` owns a persistent
//! texture that accumulates draws across calls, same as a real `<canvas>`
//! backing bitmap. Starts fully transparent, like the real spec.
//!
//! Scoped to solid-color/2-stop-gradient (linear or radial) rectangles:
//! `fillStyle`/`fillRect`/`clearRect` plus `strokeStyle`/`lineWidth`/
//! `strokeRect`, `save`/`restore`, `translate`, a `fillStyle` gradient via
//! `createLinearGradient`/`createRadialGradient` (see
//! [`LinearGradient`]/[`RadialGradient`]'s own docs for their exact scope
//! cuts), and a convex-only filled path
//! (`beginPath`/`moveTo`/`lineTo`/`arc`/`closePath`/`fill` - see
//! [`Canvas2D::fill`]'s own doc for why only convex polygons render
//! correctly and [`Canvas2D::arc`]'s own doc for its polyline-
//! approximation scope cut), a real `ctx.font`
//! (see [`Canvas2D::set_font`]'s own doc for its parser scope cut), and
//! one-line `fillText`/`strokeText`/`measureText` (see
//! [`Canvas2D::fill_text`]/[`Canvas2D::stroke_text`]'s own docs - no
//! wrapping, `strokeText` isn't a real outline stroke). No stroking a
//! path (only `strokeRect`'s rectangle-outline shortcut), no
//! `bezierCurveTo`/`quadraticCurveTo`, no drawImage sources
//! beyond another `<canvas>`, no conic gradients or patterns, no
//! `scale`/`rotate`/general transform matrix (just plain translation), no
//! compositing modes beyond `fillRect`'s source-over and
//! `clearRect`'s hard replace-with-transparent. `to_data_url` is real PNG
//! encoding (`image_decode::encode_png`) plus base64, not a stub - see
//! [`Canvas2D::to_data_url`]'s own doc for its one scope cut (PNG only,
//! `image/jpeg`/`image/webp` requests silently fall back to PNG).
//!
//! Split into one file per concern, each holding its own `impl Canvas2D`
//! block: `pipeline` (GPU vertex layouts/shaders/device setup),
//! `helpers` (pure color/coordinate math), `state` (`save`/`restore` and
//! the plain scalar setters/getters), `shapes` (solid-color rects),
//! `gradients` (`fillStyle` gradients), `path` (convex filled paths),
//! `text` (`ctx.font`/text methods), `pixels` (pixel readback/write and
//! PNG export). All are private submodules with `pub(super)` items only
//! where a sibling file needs them - the type itself and its full method
//! set are re-exported unchanged from here, so nothing outside this
//! directory needs to know it's split up.
use layout_engine::{Color, FontFamily, GenericFontFamily};

mod gradients;
mod helpers;
mod path;
mod pipeline;
mod pixels;
mod shapes;
mod state;
mod text;

pub use gradients::{FillGradient, LinearGradient, RadialGradient};

use state::CanvasState;

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
    /// Exact per-pixel circular gradient - see [`RadialGradient`]'s own
    /// doc for why this needs a dedicated pipeline/shader rather than
    /// reusing `fill_pipeline`'s 4-corner-color trick.
    radial_pipeline: wgpu::RenderPipeline,
    width: u32,
    height: u32,
    fill_style: Color,
    stroke_style: Color,
    line_width: f32,
    /// `ctx.font`'s backing - see [`Canvas2D::set_font`]'s own doc for the
    /// parser's scope cut.
    font_size: f32,
    font_family: FontFamily,
    /// `ctx.fillStyle = gradient`'s backing, when set - overrides
    /// `fill_style` for `fill_rect` only (`clearRect`/`strokeRect` stay
    /// solid-color; real spec lets any of them use a gradient, this crate
    /// only wires the one most common case). `set_fill_style` clears this
    /// back to `None`, matching real spec's "assigning `fillStyle` replaces
    /// whatever was there before, solid or gradient".
    fill_gradient: Option<FillGradient>,
    /// `ctx.translate(x, y)`'s accumulated offset - the one transform this
    /// crate supports (no scale/rotate/general matrix - see
    /// [`Canvas2D::translate`]'s own doc). Added to every `fillRect`/
    /// `clearRect`/`strokeRect` coordinate before painting.
    translate_x: f32,
    translate_y: f32,
    /// `ctx.save()`/`ctx.restore()`'s backing stack - scoped to just the
    /// drawing-state fields this crate actually has (`fillStyle`/
    /// `strokeStyle`/`lineWidth`/`font`/translate offset), not real spec's
    /// full state (no clip region or general transform matrix exists here
    /// to save).
    state_stack: Vec<CanvasState>,
    /// `beginPath`/`moveTo`/`lineTo`'s backing point list - not part of
    /// `save`/`restore`'s state stack, matching real spec (the current
    /// path is its own separate piece of state, untouched by save/
    /// restore). `moveTo` and `lineTo` are functionally identical here -
    /// both just append a point; real spec's subpath-boundary distinction
    /// isn't modeled, since this crate only ever fills one polygon per
    /// `beginPath`, not multiple subpaths.
    path_points: Vec<(f32, f32)>,
}

impl Canvas2D {
    pub fn new(width: u32, height: u32) -> Self {
        pollster::block_on(Self::new_async(width, height))
    }

    async fn new_async(width: u32, height: u32) -> Self {
        let pipeline::GpuResources {
            device,
            queue,
            texture,
            fill_pipeline,
            clear_pipeline,
            radial_pipeline,
        } = pipeline::build(width, height).await;

        let mut canvas = Canvas2D {
            device,
            queue,
            texture,
            fill_pipeline,
            clear_pipeline,
            radial_pipeline,
            width,
            height,
            fill_style: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            stroke_style: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            line_width: 1.0,
            font_size: text::DEFAULT_FONT_SIZE,
            font_family: FontFamily::generic(GenericFontFamily::SansSerif),
            fill_gradient: None,
            translate_x: 0.0,
            translate_y: 0.0,
            state_stack: Vec::new(),
            path_points: Vec::new(),
        };
        // The real spec starts a canvas fully transparent, not undefined.
        canvas.clear_rect(0.0, 0.0, width as f32, height as f32);
        canvas
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}
