mod common;

use common::pixel;
use layout_engine::Color;
use render::Canvas2D;

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
