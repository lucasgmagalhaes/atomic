mod common;

use common::pixel;
use layout_engine::Color;
use render::Canvas2D;

#[test]
fn starts_fully_transparent() {
    let canvas = Canvas2D::new(4, 4);
    let pixels = canvas.get_image_data();
    assert_eq!(pixels.len(), 4 * 4 * 4);
    assert_eq!(pixel(&pixels, 4, 0, 0), [0, 0, 0, 0]);
}

#[test]
fn fill_rect_paints_the_given_region_only() {
    let mut canvas = Canvas2D::new(8, 8);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.fill_rect(2.0, 2.0, 4.0, 4.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 8, 4, 4), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 8, 0, 0), [0, 0, 0, 0]);
    assert_eq!(pixel(&pixels, 8, 7, 7), [0, 0, 0, 0]);
}

#[test]
fn draws_accumulate_across_calls() {
    let mut canvas = Canvas2D::new(8, 8);
    canvas.set_fill_style(Color {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    });
    canvas.fill_rect(0.0, 0.0, 8.0, 8.0);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.fill_rect(0.0, 0.0, 4.0, 4.0);

    let pixels = canvas.get_image_data();
    // Overlap region: red (drawn second).
    assert_eq!(pixel(&pixels, 8, 1, 1), [255, 0, 0, 255]);
    // Untouched region: still blue from the first draw.
    assert_eq!(pixel(&pixels, 8, 6, 6), [0, 0, 255, 255]);
}

#[test]
fn clear_rect_erases_to_transparent_regardless_of_fill_style() {
    let mut canvas = Canvas2D::new(8, 8);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.fill_rect(0.0, 0.0, 8.0, 8.0);
    canvas.clear_rect(2.0, 2.0, 4.0, 4.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 8, 4, 4), [0, 0, 0, 0]);
    // Outside the cleared region: still red.
    assert_eq!(pixel(&pixels, 8, 0, 0), [255, 0, 0, 255]);
}

#[test]
fn fill_style_change_only_affects_subsequent_fill_rects() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.fill_rect(0.0, 0.0, 2.0, 2.0);
    canvas.set_fill_style(Color {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    });
    canvas.fill_rect(2.0, 2.0, 2.0, 2.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 4, 0, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(&pixels, 4, 3, 3), [0, 0, 255, 255]);
}

#[test]
fn width_height_and_fill_style_getters_round_trip() {
    let mut canvas = Canvas2D::new(10, 20);
    assert_eq!(canvas.width(), 10);
    assert_eq!(canvas.height(), 20);

    let color = Color {
        r: 12,
        g: 34,
        b: 56,
        a: 255,
    };
    canvas.set_fill_style(color);
    assert_eq!(canvas.fill_style(), color);
}

#[test]
fn stroke_rect_paints_only_the_border_not_the_interior() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_stroke_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.set_line_width(2.0);
    canvas.stroke_rect(2.0, 2.0, 6.0, 6.0);

    let pixels = canvas.get_image_data();
    // Right on the top edge: stroked.
    assert_eq!(pixel(&pixels, 10, 5, 2), [0, 255, 0, 255]);
    // Center of the rect: untouched (still transparent).
    assert_eq!(pixel(&pixels, 10, 5, 5), [0, 0, 0, 0]);
    // Outside the rect entirely: untouched.
    assert_eq!(pixel(&pixels, 10, 0, 0), [0, 0, 0, 0]);
}

#[test]
fn stroke_rect_uses_stroke_style_not_fill_style() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.set_stroke_style(Color {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    });
    canvas.set_line_width(2.0);
    canvas.stroke_rect(2.0, 2.0, 6.0, 6.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 10, 5, 2), [0, 0, 255, 255]);
}

#[test]
fn stroke_rect_line_width_widens_the_border() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_stroke_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.set_line_width(4.0);
    canvas.stroke_rect(3.0, 3.0, 4.0, 4.0);

    let pixels = canvas.get_image_data();
    // With a 4px-wide stroke centered on the edge at y=3, y=4 (two px
    // inside the nominal edge) is still part of the stroke.
    assert_eq!(pixel(&pixels, 10, 5, 4), [0, 255, 0, 255]);
}

#[test]
fn line_width_and_stroke_style_getters_round_trip() {
    let mut canvas = Canvas2D::new(10, 10);
    assert_eq!(canvas.line_width(), 1.0);
    canvas.set_line_width(5.0);
    assert_eq!(canvas.line_width(), 5.0);

    let color = Color {
        r: 9,
        g: 8,
        b: 7,
        a: 255,
    };
    canvas.set_stroke_style(color);
    assert_eq!(canvas.stroke_style(), color);
}
