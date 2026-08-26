use std::io::Cursor;

use image::{DynamicImage, ImageFormat, RgbaImage};

fn encode_test_png(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    let img = RgbaImage::from_fn(width, height, |_, _| image::Rgba(pixel));
    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encoding a test PNG should succeed");
    bytes
}

#[test]
fn decodes_a_real_png_with_correct_dimensions_and_pixels() {
    let png_bytes = encode_test_png(3, 2, [10, 20, 30, 255]);
    let decoded = image_decode::decode(&png_bytes).expect("a real PNG should decode");

    assert_eq!(decoded.width, 3);
    assert_eq!(decoded.height, 2);
    assert_eq!(decoded.rgba.len(), 3 * 2 * 4);
    // Every pixel was encoded the same solid color - spot-check a few.
    assert_eq!(&decoded.rgba[0..4], &[10, 20, 30, 255]);
    assert_eq!(&decoded.rgba[4..8], &[10, 20, 30, 255]);
    let last = decoded.rgba.len() - 4;
    assert_eq!(&decoded.rgba[last..], &[10, 20, 30, 255]);
}

#[test]
fn decodes_a_png_with_distinct_pixels_in_the_right_positions() {
    let img = RgbaImage::from_fn(2, 2, |x, y| {
        if x == 0 && y == 0 {
            image::Rgba([255, 0, 0, 255])
        } else {
            image::Rgba([0, 0, 255, 255])
        }
    });
    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();

    let decoded = image_decode::decode(&bytes).expect("should decode");
    assert_eq!(decoded.width, 2);
    assert_eq!(decoded.height, 2);
    // Row-major: pixel (0,0) is bytes 0..4, (1,0) is bytes 4..8.
    assert_eq!(&decoded.rgba[0..4], &[255, 0, 0, 255]);
    assert_eq!(&decoded.rgba[4..8], &[0, 0, 255, 255]);
}

#[test]
fn returns_none_for_garbage_bytes() {
    assert!(image_decode::decode(b"not an image, just some bytes").is_none());
}

#[test]
fn returns_none_for_empty_bytes() {
    assert!(image_decode::decode(&[]).is_none());
}

#[test]
fn decodes_a_real_jpeg() {
    // JPEG is lossy, so this doesn't assert exact pixel values - just
    // that the "jpeg" feature genuinely decodes a real encoded JPEG
    // (built via the `image` crate's own JPEG encoder, not a mock) with
    // the right dimensions.
    let img = RgbaImage::from_fn(4, 4, |_, _| image::Rgba([200, 50, 100, 255]));
    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Jpeg)
        .expect("encoding a test JPEG should succeed");

    let decoded = image_decode::decode(&bytes).expect("a real JPEG should decode");
    assert_eq!(decoded.width, 4);
    assert_eq!(decoded.height, 4);
    assert_eq!(decoded.rgba.len(), 4 * 4 * 4);
}
