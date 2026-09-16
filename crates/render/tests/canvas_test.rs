use layout_engine::Color;
use render::{Canvas2D, FillGradient, LinearGradient, RadialGradient};

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y * width + x) * 4) as usize;
    [
        pixels[idx],
        pixels[idx + 1],
        pixels[idx + 2],
        pixels[idx + 3],
    ]
}

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
fn put_image_data_writes_pixels_directly_at_the_given_offset() {
    let mut canvas = Canvas2D::new(8, 8);
    // A 2x2 solid-red region.
    let rgba = vec![255u8, 0, 0, 255].repeat(4);
    canvas.put_image_data(3, 3, 2, 2, &rgba);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 8, 3, 3), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 8, 4, 4), [255, 0, 0, 255]);
    // Outside the written region: still transparent.
    assert_eq!(pixel(&pixels, 8, 0, 0), [0, 0, 0, 0]);
    assert_eq!(pixel(&pixels, 8, 5, 5), [0, 0, 0, 0]);
}

#[test]
fn put_image_data_overwrites_rather_than_blends() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.fill_rect(0.0, 0.0, 4.0, 4.0);

    // A fully-transparent pixel written on top should replace the green,
    // not blend with it (matches clear_rect's own REPLACE semantics).
    let rgba = vec![0u8, 0, 0, 0];
    canvas.put_image_data(1, 1, 1, 1, &rgba);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 4, 1, 1), [0, 0, 0, 0]);
    assert_eq!(pixel(&pixels, 4, 0, 0), [0, 255, 0, 255]);
}

#[test]
fn put_image_data_clips_a_region_that_spills_past_the_canvas_edge() {
    let mut canvas = Canvas2D::new(4, 4);
    // A 4x4 solid-blue region positioned so half of it spills past the
    // right/bottom edge - should not panic, and the in-bounds part should
    // still land correctly.
    let rgba = vec![0u8, 0, 255, 255].repeat(16);
    canvas.put_image_data(2, 2, 4, 4, &rgba);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 4, 2, 2), [0, 0, 255, 255]);
    assert_eq!(pixel(&pixels, 4, 3, 3), [0, 0, 255, 255]);
    assert_eq!(pixel(&pixels, 4, 0, 0), [0, 0, 0, 0]);
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
fn linear_gradient_is_mostly_start_color_near_the_gradient_line_start() {
    // A wide canvas so the near-start pixel's own center is close enough to
    // t=0 that its blend-with-end-color fraction is negligible - the exact
    // corner vertex color (t=0) isn't what any *pixel* shows, since a pixel
    // center sits half a pixel in from the rect's true edge.
    let mut canvas = Canvas2D::new(200, 1);
    canvas.set_fill_gradient(FillGradient::Linear(LinearGradient {
        x0: 0.0,
        y0: 0.0,
        x1: 200.0,
        y1: 0.0,
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
    canvas.fill_rect(0.0, 0.0, 200.0, 1.0);

    let pixels = canvas.get_image_data();
    let [r, g, b, a] = pixel(&pixels, 200, 0, 0);
    assert!(r > 250, "expected near-start pixel mostly red, got r={r}");
    assert_eq!(g, 0);
    assert!(b < 10, "expected near-start pixel barely blue, got b={b}");
    assert_eq!(a, 255);
}

#[test]
fn linear_gradient_is_mostly_end_color_near_the_gradient_line_end() {
    let mut canvas = Canvas2D::new(200, 1);
    canvas.set_fill_gradient(FillGradient::Linear(LinearGradient {
        x0: 0.0,
        y0: 0.0,
        x1: 200.0,
        y1: 0.0,
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
    canvas.fill_rect(0.0, 0.0, 200.0, 1.0);

    let pixels = canvas.get_image_data();
    let [r, g, b, a] = pixel(&pixels, 200, 199, 0);
    assert!(b > 250, "expected near-end pixel mostly blue, got b={b}");
    assert_eq!(g, 0);
    assert!(r < 10, "expected near-end pixel barely red, got r={r}");
    assert_eq!(a, 255);
}

#[test]
fn set_fill_style_after_a_gradient_reverts_to_solid_color() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_fill_gradient(FillGradient::Linear(LinearGradient {
        x0: 0.0,
        y0: 0.0,
        x1: 4.0,
        y1: 0.0,
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
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    assert!(canvas.fill_gradient().is_none());
    canvas.fill_rect(0.0, 0.0, 4.0, 4.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 4, 0, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(&pixels, 4, 3, 3), [0, 255, 0, 255]);
}

#[test]
fn restore_undoes_a_gradient_fill_style_set_since_the_matching_save() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.save();
    canvas.set_fill_gradient(FillGradient::Linear(LinearGradient {
        x0: 0.0,
        y0: 0.0,
        x1: 4.0,
        y1: 0.0,
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
    canvas.restore();
    assert!(canvas.fill_gradient().is_none());
    canvas.fill_rect(0.0, 0.0, 4.0, 4.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 4, 0, 0), [0, 255, 0, 255]);
}

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
fn to_data_url_produces_a_real_decodable_png() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_fill_style(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    });
    canvas.fill_rect(0.0, 0.0, 4.0, 4.0);

    let url = canvas.to_data_url();
    let prefix = "data:image/png;base64,";
    assert!(url.starts_with(prefix), "unexpected url: {url}");

    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&url[prefix.len()..])
        .expect("valid base64");
    let decoded = image_decode::decode(&bytes).expect("valid PNG");
    assert_eq!(decoded.width, 4);
    assert_eq!(decoded.height, 4);
    assert_eq!(&decoded.rgba[0..4], &[255, 0, 0, 255]);
}

#[test]
fn to_png_bytes_is_the_same_image_to_data_url_base64_encodes() {
    let mut canvas = Canvas2D::new(4, 4);
    canvas.set_fill_style(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    });
    canvas.fill_rect(0.0, 0.0, 4.0, 4.0);

    let bytes = canvas.to_png_bytes();
    let decoded = image_decode::decode(&bytes).expect("valid PNG");
    assert_eq!(decoded.width, 4);
    assert_eq!(decoded.height, 4);
    assert_eq!(&decoded.rgba[0..4], &[0, 255, 0, 255]);
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

#[test]
fn save_and_restore_round_trip_font() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_font("16px sans-serif");
    canvas.save();
    canvas.set_font("30px monospace");
    canvas.restore();
    assert_eq!(canvas.font(), "16px sans-serif");
}

#[test]
fn radial_gradient_paints_start_color_at_its_own_center() {
    // A large radius so the sampled pixel's own center (half a pixel off
    // the true math center, same reason `linear_gradient_is_mostly_start_
    // color_near_the_gradient_line_start` needs a wide canvas) contributes
    // a negligible blend fraction.
    let mut canvas = Canvas2D::new(400, 400);
    canvas.set_fill_gradient(FillGradient::Radial(RadialGradient {
        cx: 200.0,
        cy: 200.0,
        radius: 200.0,
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
    canvas.fill_rect(0.0, 0.0, 400.0, 400.0);

    let pixels = canvas.get_image_data();
    let [r, g, b, a] = pixel(&pixels, 400, 200, 200);
    assert!(r > 250, "expected center pixel mostly red, got r={r}");
    assert_eq!(g, 0);
    assert!(b < 15, "expected center pixel barely blue, got b={b}");
    assert_eq!(a, 255);
}

#[test]
fn radial_gradient_paints_end_color_at_its_own_edge() {
    let mut canvas = Canvas2D::new(20, 20);
    canvas.set_fill_gradient(FillGradient::Radial(RadialGradient {
        cx: 10.0,
        cy: 10.0,
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
    canvas.fill_rect(0.0, 0.0, 20.0, 20.0);

    let pixels = canvas.get_image_data();
    // Corner (0,0) is at distance ~14.1 from center (10,10) - beyond the
    // radius, so clamped to the end color.
    let [r, g, b, a] = pixel(&pixels, 20, 0, 0);
    assert!(b > 200, "expected corner pixel mostly blue, got b={b}");
    assert_eq!(g, 0);
    assert!(r < 30, "expected corner pixel barely red, got r={r}");
    assert_eq!(a, 255);
}

#[test]
fn radial_gradient_is_a_true_circle_not_a_diamond() {
    // Regression guard for the exact-per-pixel-shader approach: sample
    // two points equidistant from the center along different axes (one
    // axis-aligned, one diagonal) and confirm they blend identically -
    // a naive 4-corner-color-interpolation approximation would not
    // (it'd be diamond-shaped, not circular).
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_gradient(FillGradient::Radial(RadialGradient {
        cx: 20.0,
        cy: 20.0,
        radius: 20.0,
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
    canvas.fill_rect(0.0, 0.0, 40.0, 40.0);

    let pixels = canvas.get_image_data();
    // (30, 20) is 10px right of center; (27, 27) is ~9.9px from center
    // diagonally - both should be at roughly the same blend.
    let axis = pixel(&pixels, 40, 30, 20);
    let diagonal = pixel(&pixels, 40, 27, 27);
    let diff = (axis[2] as i32 - diagonal[2] as i32).abs();
    assert!(
        diff < 15,
        "expected a true circle (similar blend at similar distance), axis={axis:?} diagonal={diagonal:?}"
    );
}

#[test]
fn set_fill_gradient_accepts_both_linear_and_radial() {
    let mut canvas = Canvas2D::new(10, 10);
    canvas.set_fill_gradient(FillGradient::Linear(LinearGradient {
        x0: 0.0,
        y0: 0.0,
        x1: 10.0,
        y1: 0.0,
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
    assert!(matches!(
        canvas.fill_gradient(),
        Some(FillGradient::Linear(_))
    ));

    canvas.set_fill_gradient(FillGradient::Radial(RadialGradient {
        cx: 5.0,
        cy: 5.0,
        radius: 5.0,
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
    assert!(matches!(
        canvas.fill_gradient(),
        Some(FillGradient::Radial(_))
    ));
}

#[test]
fn put_image_data_clips_a_negative_origin() {
    let mut canvas = Canvas2D::new(4, 4);
    // A 4x4 solid-yellow region positioned so its top-left spills off the
    // left/top edge - only the bottom-right 2x2 should actually land.
    let rgba: Vec<u8> = (0..16)
        .flat_map(|i| {
            let (row, col) = (i / 4, i % 4);
            [((row * 4 + col) * 10) as u8, 255, 0, 255]
        })
        .collect();
    canvas.put_image_data(-2, -2, 4, 4, &rgba);

    let pixels = canvas.get_image_data();
    // (0,0) on the canvas corresponds to source (2,2) - the pixel at
    // source row 2, col 2 -> index (2*4+2)=10 -> r = 100.
    assert_eq!(pixel(&pixels, 4, 0, 0), [100, 255, 0, 255]);
}
