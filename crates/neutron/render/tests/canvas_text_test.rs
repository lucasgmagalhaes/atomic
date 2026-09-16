use layout_engine::Color;
use render::Canvas2D;

#[test]
fn fill_text_paints_non_background_pixels() {
    let mut canvas = Canvas2D::new(60, 30);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.fill_text("Hi", 5.0, 5.0);

    let pixels = canvas.get_image_data();
    let any_painted = pixels.chunks(4).any(|px| px[3] != 0);
    assert!(any_painted, "expected fillText to paint at least one pixel");
}

#[test]
fn fill_text_with_empty_string_does_nothing() {
    let mut canvas = Canvas2D::new(20, 20);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.fill_text("", 5.0, 5.0);

    let pixels = canvas.get_image_data();
    assert!(pixels.chunks(4).all(|px| px[3] == 0));
}

#[test]
fn set_font_and_getter_round_trip() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_font("20px monospace");
    assert_eq!(canvas.font(), "20px monospace");
}

#[test]
fn set_font_with_a_named_family_round_trips() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_font("14px Arial, sans-serif");
    assert_eq!(canvas.font(), "14px Arial, sans-serif");
}

#[test]
fn set_font_with_an_invalid_string_is_ignored() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_font("bogus");
    assert_eq!(canvas.font(), "16px sans-serif");
}

#[test]
fn measure_text_returns_a_positive_width_for_nonempty_text() {
    let canvas = Canvas2D::new(100, 20);
    assert_eq!(canvas.measure_text(""), 0.0);
    assert!(canvas.measure_text("Hello") > 0.0);
}

#[test]
fn measure_text_grows_with_a_larger_font_size() {
    let mut canvas = Canvas2D::new(200, 40);
    let small = canvas.measure_text("Hello");
    canvas.set_font("40px sans-serif");
    let large = canvas.measure_text("Hello");
    assert!(large > small, "expected larger font to measure wider text");
}

#[test]
fn stroke_text_paints_using_stroke_style_not_fill_style() {
    let mut canvas = Canvas2D::new(60, 30);
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
    canvas.stroke_text("Hi", 5.0, 5.0);

    let pixels = canvas.get_image_data();
    let any_red = pixels
        .chunks(4)
        .any(|px| px[0] == 255 && px[2] == 0 && px[3] != 0);
    let any_blue = pixels
        .chunks(4)
        .any(|px| px[2] == 255 && px[0] == 0 && px[3] != 0);
    assert!(!any_red, "strokeText should not paint fillStyle color");
    assert!(any_blue, "expected strokeText to paint strokeStyle color");
}
