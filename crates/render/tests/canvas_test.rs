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
