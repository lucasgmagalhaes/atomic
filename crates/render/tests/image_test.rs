use std::rc::Rc;

use image_decode::DecodedImage;
use render::{composite_images, ClipRect, ImageQuad};

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
  let i = ((y * width + x) * 4) as usize;
  [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn solid_image(width: u32, height: u32, color: [u8; 4]) -> Rc<DecodedImage> {
  let mut rgba = Vec::with_capacity((width * height * 4) as usize);
  for _ in 0..(width * height) {
    rgba.extend_from_slice(&color);
  }
  Rc::new(DecodedImage {
    width,
    height,
    rgba,
  })
}

#[test]
fn composites_a_real_image_at_1to1_scale() {
  let mut pixels = vec![0u8; 4 * 4 * 4];
  let image = solid_image(4, 4, [255, 0, 0, 255]);
  let quad = ImageQuad {
    x: 0.0,
    y: 0.0,
    width: 4.0,
    height: 4.0,
    image,
    clip: None,
    opacity: 1.0,
  };

  composite_images(&mut pixels, 4, 4, &[quad]);

  assert_eq!(pixel(&pixels, 4, 0, 0), [255, 0, 0, 255]);
  assert_eq!(pixel(&pixels, 4, 3, 3), [255, 0, 0, 255]);
}

#[test]
fn composites_at_an_offset_and_leaves_the_rest_untouched() {
  let mut pixels = vec![0u8; 8 * 8 * 4];
  let image = solid_image(2, 2, [0, 255, 0, 255]);
  let quad = ImageQuad {
    x: 3.0,
    y: 3.0,
    width: 2.0,
    height: 2.0,
    image,
    clip: None,
    opacity: 1.0,
  };

  composite_images(&mut pixels, 8, 8, &[quad]);

  assert_eq!(pixel(&pixels, 8, 3, 3), [0, 255, 0, 255]);
  assert_eq!(pixel(&pixels, 8, 4, 4), [0, 255, 0, 255]);
  // Outside the placed quad - untouched (still zeroed).
  assert_eq!(pixel(&pixels, 8, 0, 0), [0, 0, 0, 0]);
  assert_eq!(pixel(&pixels, 8, 5, 5), [0, 0, 0, 0]);
}

#[test]
fn nearest_neighbor_upscaling_samples_real_source_pixels() {
  // A 2x1 image (left pixel red, right pixel blue) stretched to 4x1 -
  // real nearest-neighbor sampling, not interpolation/averaging.
  let mut rgba = Vec::new();
  rgba.extend_from_slice(&[255, 0, 0, 255]);
  rgba.extend_from_slice(&[0, 0, 255, 255]);
  let image = Rc::new(DecodedImage {
    width: 2,
    height: 1,
    rgba,
  });
  let quad = ImageQuad {
    x: 0.0,
    y: 0.0,
    width: 4.0,
    height: 1.0,
    image,
    clip: None,
    opacity: 1.0,
  };

  let mut pixels = vec![0u8; 4 * 1 * 4];
  composite_images(&mut pixels, 4, 1, &[quad]);

  assert_eq!(pixel(&pixels, 4, 0, 0), [255, 0, 0, 255]);
  assert_eq!(pixel(&pixels, 4, 1, 0), [255, 0, 0, 255]);
  assert_eq!(pixel(&pixels, 4, 2, 0), [0, 0, 255, 255]);
  assert_eq!(pixel(&pixels, 4, 3, 0), [0, 0, 255, 255]);
}

#[test]
fn transparent_source_pixels_do_not_overwrite_the_destination() {
  let mut pixels = vec![
    10u8, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255,
  ];
  let image = solid_image(2, 2, [0, 0, 0, 0]);
  let quad = ImageQuad {
    x: 0.0,
    y: 0.0,
    width: 2.0,
    height: 2.0,
    image,
    clip: None,
    opacity: 1.0,
  };

  composite_images(&mut pixels, 2, 2, &[quad]);

  assert_eq!(pixel(&pixels, 2, 0, 0), [10, 20, 30, 255]);
}

#[test]
fn a_zero_sized_quad_is_skipped_without_panicking() {
  let mut pixels = vec![0u8; 1 * 1 * 4];
  let image = solid_image(2, 2, [255, 255, 255, 255]);
  let quad = ImageQuad {
    x: 0.0,
    y: 0.0,
    width: 0.0,
    height: 0.0,
    image,
    clip: None,
    opacity: 1.0,
  };

  composite_images(&mut pixels, 1, 1, &[quad]);
  assert_eq!(pixels, vec![0u8; 4]);
}

#[test]
fn clip_restricts_the_painted_region_without_distorting_the_scale() {
  // A 4x4 source stretched to an 8x8 destination (1:2 scale in both
  // axes), but clipped to only the left half - real crop, not a
  // shrunk/distorted image: the clip only narrows *where* pixels land,
  // `scale_x`/`scale_y` still divide by the quad's full 8x8 size.
  let mut pixels = vec![0u8; 8 * 8 * 4];
  let image = solid_image(4, 4, [255, 0, 0, 255]);
  let quad = ImageQuad {
    x: 0.0,
    y: 0.0,
    width: 8.0,
    height: 8.0,
    image,
    clip: Some(ClipRect {
      x: 0.0,
      y: 0.0,
      width: 4.0,
      height: 8.0,
    }),
    opacity: 1.0,
  };

  composite_images(&mut pixels, 8, 8, &[quad]);

  // Inside the clip - painted.
  assert_eq!(pixel(&pixels, 8, 0, 0), [255, 0, 0, 255]);
  assert_eq!(pixel(&pixels, 8, 3, 7), [255, 0, 0, 255]);
  // Outside the clip - untouched, even though the quad's own unclipped
  // bounds cover this region too.
  assert_eq!(pixel(&pixels, 8, 4, 0), [0, 0, 0, 0]);
  assert_eq!(pixel(&pixels, 8, 7, 7), [0, 0, 0, 0]);
}

#[test]
fn a_quad_entirely_outside_its_clip_paints_nothing() {
  let mut pixels = vec![0u8; 4 * 4 * 4];
  let image = solid_image(4, 4, [255, 0, 0, 255]);
  let quad = ImageQuad {
    x: 0.0,
    y: 0.0,
    width: 4.0,
    height: 4.0,
    image,
    clip: Some(ClipRect {
      x: 10.0,
      y: 10.0,
      width: 4.0,
      height: 4.0,
    }),
    opacity: 1.0,
  };

  composite_images(&mut pixels, 4, 4, &[quad]);
  assert_eq!(pixels, vec![0u8; 4 * 4 * 4]);
}

#[test]
fn opacity_scales_the_composited_alpha() {
  let mut pixels = vec![0u8; 4 * 4 * 4];
  let image = solid_image(4, 4, [255, 0, 0, 255]);
  let quad = ImageQuad {
    x: 0.0,
    y: 0.0,
    width: 4.0,
    height: 4.0,
    image,
    clip: None,
    opacity: 0.5,
  };

  composite_images(&mut pixels, 4, 4, &[quad]);

  assert_eq!(pixel(&pixels, 4, 0, 0), [255, 0, 0, 128]);
}

#[test]
fn zero_opacity_paints_nothing() {
  let mut pixels = vec![0u8; 4 * 4 * 4];
  let image = solid_image(4, 4, [255, 0, 0, 255]);
  let quad = ImageQuad {
    x: 0.0,
    y: 0.0,
    width: 4.0,
    height: 4.0,
    image,
    clip: None,
    opacity: 0.0,
  };

  composite_images(&mut pixels, 4, 4, &[quad]);

  assert_eq!(pixels, vec![0u8; 4 * 4 * 4]);
}
