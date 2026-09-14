use layout_engine::Color;
use render::{list_adapters, GpuRenderer, Rect};

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
        color: Color {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        radius: 0.0,
        gradient: None,
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
        Rect {
            x: 0.0,
            y: 0.0,
            width: 8.0,
            height: 8.0,
            color: Color {
                r: 0,
                g: 0,
                b: 255,
                a: 255,
            },
            radius: 0.0,
            gradient: None,
        },
        Rect {
            x: 0.0,
            y: 0.0,
            width: 4.0,
            height: 4.0,
            color: Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            radius: 0.0,
            gradient: None,
        },
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

#[test]
fn list_adapters_returns_at_least_one_real_adapter_with_a_name() {
    let adapters = list_adapters();
    assert!(
        !adapters.is_empty(),
        "this machine should have at least one real wgpu adapter (GPU or software fallback)"
    );
    for adapter in &adapters {
        assert!(
            !adapter.name.is_empty(),
            "a real adapter should report a non-empty name, got {adapter:?}"
        );
        assert!(
            !adapter.backend.is_empty(),
            "a real adapter should report a non-empty backend, got {adapter:?}"
        );
    }
}

#[test]
fn new_with_adapter_opens_the_first_enumerated_adapter_and_renders_correctly() {
    let adapters = list_adapters();
    assert!(!adapters.is_empty());

    let renderer = GpuRenderer::new_with_adapter(0);
    let pixels = renderer.render_to_rgba(&[], 4, 4, [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(
        pixel(&pixels, 4, 0, 0),
        [0, 255, 0, 255],
        "explicit adapter selection should still render correctly"
    );
}

const RED: Color = Color {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};
const BLUE: Color = Color {
    r: 0,
    g: 0,
    b: 255,
    a: 255,
};

#[test]
fn a_90_degree_gradient_varies_only_left_to_right() {
    let renderer = GpuRenderer::new();
    let rects = [Rect {
        x: 0.0,
        y: 0.0,
        width: 8.0,
        height: 8.0,
        color: RED,
        radius: 0.0,
        gradient: Some((RED, BLUE, 90.0)),
    }];
    let pixels = renderer.render_to_rgba(&rects, 8, 8, [0.0, 0.0, 0.0, 1.0]);

    let left = pixel(&pixels, 8, 0, 4);
    let right = pixel(&pixels, 8, 7, 4);
    assert!(
        left[0] > right[0],
        "red channel should decrease left to right, got {left:?} -> {right:?}"
    );
    assert!(
        left[2] < right[2],
        "blue channel should increase left to right, got {left:?} -> {right:?}"
    );
    // A 90deg (left-to-right) gradient doesn't vary with y at all - the
    // same column should paint identically regardless of row.
    assert_eq!(
        pixel(&pixels, 8, 0, 0),
        pixel(&pixels, 8, 0, 7),
        "a horizontal gradient must not vary vertically"
    );
}

#[test]
fn a_gradient_with_no_direction_defaults_to_top_to_bottom() {
    let renderer = GpuRenderer::new();
    let rects = [Rect {
        x: 0.0,
        y: 0.0,
        width: 8.0,
        height: 8.0,
        color: RED,
        radius: 0.0,
        gradient: Some((RED, BLUE, 180.0)),
    }];
    let pixels = renderer.render_to_rgba(&rects, 8, 8, [0.0, 0.0, 0.0, 1.0]);

    let top = pixel(&pixels, 8, 4, 0);
    let bottom = pixel(&pixels, 8, 4, 7);
    assert!(
        top[0] > bottom[0],
        "red channel should decrease top to bottom, got {top:?} -> {bottom:?}"
    );
    assert!(
        top[2] < bottom[2],
        "blue channel should increase top to bottom, got {top:?} -> {bottom:?}"
    );
    assert_eq!(
        pixel(&pixels, 8, 0, 0),
        pixel(&pixels, 8, 7, 0),
        "a vertical (180deg) gradient must not vary horizontally"
    );
}
