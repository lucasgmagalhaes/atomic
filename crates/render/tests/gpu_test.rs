use layout_engine::Color;
use render::{GpuRenderer, Rect};

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y * width + x) * 4) as usize;
    [pixels[idx], pixels[idx + 1], pixels[idx + 2], pixels[idx + 3]]
}

#[test]
fn clears_to_the_given_color_with_no_rects() {
    let renderer = GpuRenderer::new();
    let pixels = renderer.render_to_rgba(&[], 4, 4, [0.0, 0.0, 0.0, 1.0]);

    assert_eq!(pixels.len(), 4 * 4 * 4);
    assert_eq!(pixel(&pixels, 4, 0, 0), [0, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 4, 3, 3), [0, 0, 0, 255]);
}

#[test]
fn paints_a_solid_rect_at_the_right_pixels() {
    let renderer = GpuRenderer::new();
    let rects = [Rect {
        x: 2.0,
        y: 2.0,
        width: 4.0,
        height: 4.0,
        color: Color { r: 255, g: 0, b: 0, a: 255 },
    }];
    let pixels = renderer.render_to_rgba(&rects, 8, 8, [0.0, 0.0, 0.0, 1.0]);

    // Inside the rect: red.
    assert_eq!(pixel(&pixels, 8, 4, 4), [255, 0, 0, 255]);
    // Outside the rect: still the clear color.
    assert_eq!(pixel(&pixels, 8, 0, 0), [0, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 8, 7, 7), [0, 0, 0, 255]);
}

#[test]
fn later_rects_paint_over_earlier_ones_at_the_same_pixel() {
    let renderer = GpuRenderer::new();
    let rects = [
        Rect { x: 0.0, y: 0.0, width: 8.0, height: 8.0, color: Color { r: 0, g: 0, b: 255, a: 255 } },
        Rect { x: 0.0, y: 0.0, width: 4.0, height: 4.0, color: Color { r: 255, g: 0, b: 0, a: 255 } },
    ];
    let pixels = renderer.render_to_rgba(&rects, 8, 8, [0.0, 0.0, 0.0, 1.0]);

    // Overlapping region: red (painted second) wins.
    assert_eq!(pixel(&pixels, 8, 1, 1), [255, 0, 0, 255]);
    // Non-overlapping region of the first rect: still blue.
    assert_eq!(pixel(&pixels, 8, 6, 6), [0, 0, 255, 255]);
}

#[test]
fn output_size_matches_requested_dimensions() {
    let renderer = GpuRenderer::new();
    // A non-multiple-of-64 width to exercise the row-padding/unpadding
    // path (wgpu requires 256-byte-aligned copy rows; width=5 -> 20
    // bytes/row unpadded, which is not a multiple of 256).
    let pixels = renderer.render_to_rgba(&[], 5, 3, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(pixels.len(), 5 * 3 * 4);
    assert_eq!(pixel(&pixels, 5, 4, 2), [255, 255, 255, 255]);
}
