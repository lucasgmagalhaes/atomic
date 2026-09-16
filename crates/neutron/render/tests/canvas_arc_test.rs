//! `ctx.arc()` - `render::Canvas2D::arc`'s polyline-approximation scope.
mod common;

use common::pixel;
use layout_engine::Color;
use render::Canvas2D;

#[test]
fn arc_fills_a_full_circle() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.begin_path();
    canvas.arc(20.0, 20.0, 15.0, 0.0, std::f32::consts::PI * 2.0, false);
    canvas.fill();

    let pixels = canvas.get_image_data();
    // Center of the circle: painted.
    assert_eq!(pixel(&pixels, 40, 20, 20), [0, 255, 0, 255]);
    // Far corner, well outside the circle: untouched.
    assert_eq!(pixel(&pixels, 40, 1, 1), [0, 0, 0, 0]);
}

#[test]
fn arc_with_a_partial_sweep_leaves_the_opposite_side_untouched() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.begin_path();
    canvas.move_to(20.0, 20.0);
    // A quarter-circle sweep from angle 0 to PI/2 (right side, going down).
    canvas.arc(20.0, 20.0, 15.0, 0.0, std::f32::consts::FRAC_PI_2, false);
    canvas.close_path();
    canvas.fill();

    let pixels = canvas.get_image_data();
    // Inside the quarter-circle wedge (down-right of center).
    assert_eq!(pixel(&pixels, 40, 28, 28), [255, 0, 0, 255]);
    // Up-left of center: outside the wedge, untouched.
    assert_eq!(pixel(&pixels, 40, 5, 5), [0, 0, 0, 0]);
}
