//! The solid-color-quad WGSL shader, its `Vertex` layout, and
//! `rect_to_vertices` — split out from `gpu.rs`.

use bytemuck::{Pod, Zeroable};

use crate::display_list::Rect;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Vertex {
    pub(super) position: [f32; 2],
    pub(super) color: [f32; 4],
    /// This vertex's position relative to the rect's own center, in pixel
    /// units (not NDC) — same units as `half_size_radius.xy`, so the
    /// fragment shader's distance-to-edge math doesn't need to un-do any
    /// aspect-ratio distortion from the NDC conversion `vs_main` applies
    /// to `position`.
    pub(super) local_pos: [f32; 2],
    /// `[half_width, half_height, radius]`, all in pixels, identical on
    /// every vertex of one rect — carried per-vertex rather than via a
    /// uniform/bind group since this pipeline has none yet (see the
    /// empty `bind_group_layouts` below) and every other rect attribute
    /// already travels this way.
    pub(super) half_size_radius: [f32; 3],
}

pub(super) const SHADER_SRC: &str = r#"
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) local_pos: vec2<f32>,
    @location(2) half_size_radius: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) local_pos: vec2<f32>,
    @location(3) half_size_radius: vec3<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(position, 0.0, 1.0);
    out.color = color;
    out.local_pos = local_pos;
    out.half_size_radius = half_size_radius;
    return out;
}

// Standard signed-distance field for a rounded box centered at the
// origin (Inigo Quilez's formulation): negative inside, positive
// outside, magnitude is the real distance to the nearest edge — exact
// for any radius up to `min(b.x, b.y)` (which `rect_to_vertices` already
// clamps to on the Rust side).
fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = in.half_size_radius.z;
    if (radius <= 0.0) {
        return in.color;
    }
    let dist = sd_rounded_box(in.local_pos, in.half_size_radius.xy, radius);
    // 1px-wide smoothstep band straddling the true edge (dist == 0) is
    // the standard cheap AA for an SDF edge — sharp enough at normal UI
    // sizes, no MSAA/supersampling needed.
    let alpha = 1.0 - smoothstep(-1.0, 1.0, dist);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

pub(super) fn rect_to_vertices(
    rect: &Rect,
    viewport_width: f32,
    viewport_height: f32,
) -> [Vertex; 6] {
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

    let half_w = rect.width / 2.0;
    let half_h = rect.height / 2.0;
    // A radius past either half-dimension isn't a valid rounded rect (the
    // SDF formula assumes r <= min(b.x, b.y)) - clamps to the largest
    // radius that still fits, which is exactly a pill/stadium shape on
    // the constraining axis.
    let radius = rect.radius.max(0.0).min(half_w).min(half_h);
    let half_size_radius = [half_w, half_h, radius];

    let local = |lx: f32, ly: f32| [lx, ly];

    let tl = Vertex {
        position: [x0, y0],
        color,
        local_pos: local(-half_w, -half_h),
        half_size_radius,
    };
    let tr = Vertex {
        position: [x1, y0],
        color,
        local_pos: local(half_w, -half_h),
        half_size_radius,
    };
    let bl = Vertex {
        position: [x0, y1],
        color,
        local_pos: local(-half_w, half_h),
        half_size_radius,
    };
    let br = Vertex {
        position: [x1, y1],
        color,
        local_pos: local(half_w, half_h),
        half_size_radius,
    };

    [tl, bl, tr, tr, bl, br]
}
