mod common;

use common::pixel;
use layout_engine::Color;
use render::Canvas2D;

#[test]
fn restore_undoes_state_changes_made_since_the_matching_save() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.set_line_width(1.0);
    canvas.save();
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.set_line_width(5.0);
    canvas.restore();

    assert_eq!(
        canvas.fill_style(),
        Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }
    );
    assert_eq!(canvas.line_width(), 1.0);
}

#[test]
fn restore_on_an_empty_stack_is_a_no_op() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_fill_style(Color {
        r: 9,
        g: 9,
        b: 9,
        a: 255,
    });
    canvas.restore();
    assert_eq!(
        canvas.fill_style(),
        Color {
            r: 9,
            g: 9,
            b: 9,
            a: 255,
        }
    );
}

#[test]
fn nested_save_restore_unwinds_in_lifo_order() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_line_width(1.0);
    canvas.save();
    canvas.set_line_width(2.0);
    canvas.save();
    canvas.set_line_width(3.0);
    canvas.restore();
    assert_eq!(canvas.line_width(), 2.0);
    canvas.restore();
    assert_eq!(canvas.line_width(), 1.0);
}

#[test]
fn translate_offsets_subsequent_fill_rects() {
    let mut canvas = Canvas2D::new(8, 8);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.translate(2.0, 2.0);
    canvas.fill_rect(0.0, 0.0, 2.0, 2.0);

    let pixels = canvas.get_image_data();
    // Painted at (2,2) not (0,0), because of the translate.
    assert_eq!(pixel(&pixels, 8, 2, 2), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 8, 0, 0), [0, 0, 0, 0]);
}

#[test]
fn translate_accumulates_across_repeated_calls() {
    let mut canvas = Canvas2D::new(8, 8);
    canvas.set_fill_style(Color {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    });
    canvas.translate(1.0, 1.0);
    canvas.translate(1.0, 1.0);
    canvas.fill_rect(0.0, 0.0, 1.0, 1.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 8, 2, 2), [0, 0, 255, 255]);
}

#[test]
fn restore_undoes_a_translate_made_since_the_matching_save() {
    let mut canvas = Canvas2D::new(8, 8);
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.save();
    canvas.translate(3.0, 3.0);
    canvas.restore();
    canvas.fill_rect(0.0, 0.0, 1.0, 1.0);

    let pixels = canvas.get_image_data();
    // Translate was undone by restore, so the rect lands back at (0,0).
    assert_eq!(pixel(&pixels, 8, 0, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(&pixels, 8, 3, 3), [0, 0, 0, 0]);
}

#[test]
fn save_and_restore_round_trip_font() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_font("16px sans-serif");
    canvas.save();
    canvas.set_font("30px monospace");
    canvas.restore();
    assert_eq!(canvas.font(), "16px sans-serif");
}
