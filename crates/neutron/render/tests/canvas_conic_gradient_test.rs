//! `ctx.createConicGradient()` - `render::Canvas2D`'s [`ConicGradient`]
//! fill, same real-per-pixel-shader standard `canvas_gradient_test.rs`
//! already covers for `RadialGradient`.
mod common;

use common::pixel;
use layout_engine::Color;
use render::{Canvas2D, ConicGradient, FillGradient};

const RED: Color = Color {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};
const BLUE: Color = Color {
    r: 0,
    g: 0,
    b: 255,
    a: 255,
};

#[test]
fn conic_gradient_paints_start_color_at_its_own_start_angle() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_gradient(FillGradient::Conic(ConicGradient {
        start_angle: 0.0,
        cx: 20.0,
        cy: 20.0,
        start: RED,
        end: BLUE,
    }));
    canvas.fill_rect(0.0, 0.0, 40.0, 40.0);

    let pixels = canvas.get_image_data();
    // Directly along angle 0 (positive x-axis) from center: right at the
    // gradient's own start, so this should be pure (or near-pure) red.
    let p = pixel(&pixels, 40, 38, 20);
    assert!(
        p[0] > 200 && p[2] < 60,
        "expected near-red at t=0, got {p:?}"
    );
}

#[test]
fn conic_gradient_paints_end_color_just_before_wrapping_back_to_start() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_gradient(FillGradient::Conic(ConicGradient {
        start_angle: 0.0,
        cx: 20.0,
        cy: 20.0,
        start: RED,
        end: BLUE,
    }));
    canvas.fill_rect(0.0, 0.0, 40.0, 40.0);

    let pixels = canvas.get_image_data();
    // Just *counter-clockwise* of angle 0 (dy slightly negative, since
    // this crate's y-down space makes a positive dy read as a small
    // positive/clockwise angle) - wraps to just under 2*PI, t close to
    // 1.0, near-pure blue.
    let p = pixel(&pixels, 40, 38, 19);
    assert!(
        p[2] > 200 && p[0] < 60,
        "expected near-blue at t~1, got {p:?}"
    );
}

#[test]
fn conic_gradient_is_mostly_a_blend_halfway_around() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_gradient(FillGradient::Conic(ConicGradient {
        start_angle: 0.0,
        cx: 20.0,
        cy: 20.0,
        start: RED,
        end: BLUE,
    }));
    canvas.fill_rect(0.0, 0.0, 40.0, 40.0);

    let pixels = canvas.get_image_data();
    // Directly along angle PI (negative x-axis) from center: t = 0.5,
    // roughly an even red/blue blend, clearly neither pure color.
    let p = pixel(&pixels, 40, 2, 20);
    assert!(
        p[0] > 40 && p[0] < 220,
        "expected a blend at t=0.5, got {p:?}"
    );
    assert!(
        p[2] > 40 && p[2] < 220,
        "expected a blend at t=0.5, got {p:?}"
    );
}

#[test]
fn set_fill_gradient_accepts_conic() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_fill_gradient(FillGradient::Conic(ConicGradient {
        start_angle: 0.0,
        cx: 5.0,
        cy: 5.0,
        start: RED,
        end: BLUE,
    }));
    assert!(canvas.fill_gradient().is_some());
}
