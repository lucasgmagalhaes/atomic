//! `ctx.bezierCurveTo()`/`ctx.quadraticCurveTo()` - see
//! `render::Canvas2D::bezier_curve_to`'s own doc for the polyline-
//! approximation scope, same convention `canvas_arc_test.rs` covers for
//! `arc`.
mod common;

use common::pixel;
use layout_engine::Color;
use render::Canvas2D;

const RED: Color = Color {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};

#[test]
fn bezier_curve_to_reaches_the_end_point_and_stays_inside_the_closed_shape() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(RED);
    canvas.begin_path();
    canvas.move_to(5.0, 20.0);
    // Bulges upward via the two control points, ends at (35, 20).
    canvas.bezier_curve_to(12.0, 2.0, 28.0, 2.0, 35.0, 20.0);
    canvas.line_to(35.0, 35.0);
    canvas.line_to(5.0, 35.0);
    canvas.close_path();
    canvas.fill();

    let pixels = canvas.get_image_data();
    // Well inside the closed shape, below the curve's bulge.
    assert_eq!(pixel(&pixels, 40, 20, 30), [255, 0, 0, 255]);
    // Far corner, outside the shape entirely.
    assert_eq!(pixel(&pixels, 40, 1, 1), [0, 0, 0, 0]);
}

#[test]
fn bezier_curve_to_is_a_no_op_with_no_current_point() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(RED);
    canvas.begin_path();
    // No moveTo/lineTo first - path_points is empty.
    canvas.bezier_curve_to(10.0, 10.0, 20.0, 10.0, 30.0, 20.0);
    canvas.fill();

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 40, 20, 20), [0, 0, 0, 0]);
}

#[test]
fn quadratic_curve_to_reaches_the_end_point_and_stays_inside_the_closed_shape() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(RED);
    canvas.begin_path();
    canvas.move_to(5.0, 20.0);
    canvas.quadratic_curve_to(20.0, 2.0, 35.0, 20.0);
    canvas.line_to(35.0, 35.0);
    canvas.line_to(5.0, 35.0);
    canvas.close_path();
    canvas.fill();

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 40, 20, 30), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 40, 1, 1), [0, 0, 0, 0]);
}

#[test]
fn quadratic_curve_to_is_a_no_op_with_no_current_point() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(RED);
    canvas.begin_path();
    canvas.quadratic_curve_to(20.0, 10.0, 30.0, 20.0);
    canvas.fill();

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 40, 20, 20), [0, 0, 0, 0]);
}
