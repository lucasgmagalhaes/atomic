use layout_engine::Color;
use render::Canvas2D;

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
