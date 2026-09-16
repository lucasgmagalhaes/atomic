mod common;

use common::pixel;
use layout_engine::Color;
use render::Canvas2D;

#[test]
fn fill_paints_a_triangle_from_a_path() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.begin_path();
    canvas.move_to(0.0, 0.0);
    canvas.line_to(10.0, 0.0);
    canvas.line_to(0.0, 10.0);
    canvas.close_path();
    canvas.fill();

    let pixels = canvas.get_image_data();
    // Well inside the triangle.
    assert_eq!(pixel(&pixels, 10, 1, 1), [0, 255, 0, 255]);
    // Outside the triangle (top-right corner of the square).
    assert_eq!(pixel(&pixels, 10, 9, 9), [0, 0, 0, 0]);
}

#[test]
fn fill_paints_a_square_from_a_4_point_path() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.begin_path();
    canvas.move_to(2.0, 2.0);
    canvas.line_to(8.0, 2.0);
    canvas.line_to(8.0, 8.0);
    canvas.line_to(2.0, 8.0);
    canvas.close_path();
    canvas.fill();

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 10, 5, 5), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 10, 0, 0), [0, 0, 0, 0]);
}

#[test]
fn fill_with_fewer_than_3_points_does_nothing() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.begin_path();
    canvas.move_to(0.0, 0.0);
    canvas.line_to(10.0, 10.0);
    canvas.fill();

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 10, 1, 1), [0, 0, 0, 0]);
}

#[test]
fn begin_path_discards_previously_accumulated_points() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.begin_path();
    canvas.move_to(0.0, 0.0);
    canvas.line_to(10.0, 0.0);
    canvas.line_to(0.0, 10.0);
    canvas.begin_path();
    canvas.move_to(2.0, 2.0);
    canvas.line_to(8.0, 2.0);
    canvas.line_to(8.0, 8.0);
    canvas.fill();

    let pixels = canvas.get_image_data();
    // Nothing from the discarded first path's own area (outside the
    // second path's own triangle) should have painted.
    assert_eq!(pixel(&pixels, 10, 0, 9), [0, 0, 0, 0]);
}
