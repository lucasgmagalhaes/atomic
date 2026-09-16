#![allow(dead_code)]

/// Reads one RGBA8 pixel out of a tightly-packed, row-major pixel buffer
/// (the layout `Canvas2D::get_image_data` returns) - shared by every
/// `canvas_*_test.rs` file.
pub(crate) fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y * width + x) * 4) as usize;
    [
        pixels[idx],
        pixels[idx + 1],
        pixels[idx + 2],
        pixels[idx + 3],
    ]
}
