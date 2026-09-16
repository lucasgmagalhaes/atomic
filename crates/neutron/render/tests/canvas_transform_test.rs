//! `ctx.scale()`/`ctx.rotate()`/`ctx.setTransform()`/`ctx.resetTransform()`
//! - `render::Canvas2D`'s composed CTM (`Canvas2D::transform_point`),
//! generalizing `translate` (already covered by `canvas_state_test.rs`).
mod common;

use common::pixel;
use layout_engine::Color;
use render::Canvas2D;

const RED: Color = Color {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};

#[test]
fn scale_stretches_a_rect_away_from_the_origin() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(RED);
    canvas.scale(2.0, 2.0);
    // In local space this is a 5x5 rect at (2, 2) - after 2x scale it
    // becomes a 10x10 rect at (4, 4) in canvas pixel space.
    canvas.fill_rect(2.0, 2.0, 5.0, 5.0);

    let pixels = canvas.get_image_data();
    // Inside the scaled rect (canvas space).
    assert_eq!(pixel(&pixels, 40, 8, 8), [255, 0, 0, 255]);
    // Outside it, but would have been inside the *unscaled* rect's
    // canvas-space footprint if scale were ignored.
    assert_eq!(pixel(&pixels, 40, 3, 3), [0, 0, 0, 0]);
}

#[test]
fn rotate_moves_a_rect_off_its_unrotated_position() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(RED);
    canvas.translate(20.0, 20.0);
    canvas.rotate(std::f32::consts::FRAC_PI_2);
    // A rect that, unrotated, would sit to the right of center; after a
    // 90-degree rotation it ends up below center instead.
    canvas.fill_rect(2.0, -2.0, 8.0, 4.0);

    let pixels = canvas.get_image_data();
    // Below center: painted (where the rotated rect landed).
    assert_eq!(pixel(&pixels, 40, 20, 25), [255, 0, 0, 255]);
    // To the right of center: NOT painted (where it would have been
    // without the rotation).
    assert_eq!(pixel(&pixels, 40, 25, 20), [0, 0, 0, 0]);
}

#[test]
fn set_transform_replaces_rather_than_composes() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(RED);
    canvas.translate(100.0, 100.0); // would push the rect off-canvas
    canvas.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0); // back to identity
    canvas.fill_rect(5.0, 5.0, 10.0, 10.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 40, 10, 10), [255, 0, 0, 255]);
}

#[test]
fn reset_transform_undoes_every_prior_translate_scale_and_rotate() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(RED);
    canvas.translate(15.0, 15.0);
    canvas.scale(3.0, 3.0);
    canvas.rotate(1.0);
    canvas.reset_transform();
    canvas.fill_rect(5.0, 5.0, 10.0, 10.0);

    let pixels = canvas.get_image_data();
    assert_eq!(pixel(&pixels, 40, 10, 10), [255, 0, 0, 255]);
}

#[test]
fn save_and_restore_round_trip_the_full_transform() {
    let mut canvas = Canvas2D::new(40, 40);
    canvas.set_fill_style(RED);
    canvas.save();
    canvas.scale(2.0, 2.0);
    canvas.rotate(0.5);
    canvas.restore();
    canvas.fill_rect(5.0, 5.0, 10.0, 10.0);

    let pixels = canvas.get_image_data();
    // Back to the identity transform: an unrotated, unscaled rect.
    assert_eq!(pixel(&pixels, 40, 10, 10), [255, 0, 0, 255]);
    assert_eq!(pixel(&pixels, 40, 1, 1), [0, 0, 0, 0]);
}
