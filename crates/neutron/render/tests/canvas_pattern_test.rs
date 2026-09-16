//! `ctx.createPattern()` - `render::Canvas2D`'s `Pattern` fill, CPU-side
//! tiling. See `render::canvas::patterns`'s own doc for the `'repeat'`-
//! only scope cut (real spec's `repetition` argument isn't modeled here
//! at all - this crate always tiles both axes).
mod common;

use common::pixel;
use layout_engine::Color;
use render::{Canvas2D, FillGradient, Pattern, RadialGradient};

/// A 2x1 RGBA source: pixel 0 red, pixel 1 blue.
fn two_pixel_pattern() -> Pattern {
    Pattern {
        source_width: 2,
        source_height: 1,
        source_pixels: vec![255, 0, 0, 255, 0, 0, 255, 255],
    }
}

#[test]
fn pattern_tiles_across_the_fill_rect() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_pattern(two_pixel_pattern());
    canvas.fill_rect(0.0, 0.0, 8.0, 1.0);

    let pixels = canvas.get_image_data();
    // Even x: red (source pixel 0). Odd x: blue (source pixel 1).
    // Repeats every 2 pixels across the whole 8px-wide fill.
    for x in [0, 2, 4, 6] {
        assert_eq!(pixel(&pixels, 40, x, 0), [255, 0, 0, 255], "x={x}");
    }
    for x in [1, 3, 5, 7] {
        assert_eq!(pixel(&pixels, 40, x, 0), [0, 0, 255, 255], "x={x}");
    }
}

#[test]
fn pattern_does_not_paint_outside_the_fill_rect() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_pattern(two_pixel_pattern());
    canvas.fill_rect(0.0, 0.0, 8.0, 1.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 40, 20, 20), [0, 0, 0, 0]);
}

#[test]
fn set_fill_style_after_a_pattern_reverts_to_solid_color() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_fill_pattern(two_pixel_pattern());
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.fill_rect(0.0, 0.0, 4.0, 4.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 10, 2, 2), [0, 255, 0, 255]);
}

#[test]
fn set_fill_gradient_after_a_pattern_reverts_to_gradient() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_fill_pattern(two_pixel_pattern());
    canvas.set_fill_gradient(FillGradient::Radial(RadialGradient {
        cx: 5.0,
        cy: 5.0,
        radius: 10.0,
        start: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        end: Color {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        },
    }));
    assert!(canvas.fill_gradient().is_some());
    assert!(canvas.fill_pattern().is_none());
}
