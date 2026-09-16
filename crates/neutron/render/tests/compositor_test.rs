//! `render::composite_layer_onto` (`ROADMAP.md` item 28).

use render::composite_layer_onto;

fn solid(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut buf = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        buf.extend_from_slice(&rgba);
    }
    buf
}

#[test]
fn a_fully_transparent_layer_changes_nothing() {
    let mut base = solid(2, 2, [10, 20, 30, 255]);
    let layer = solid(2, 2, [0, 0, 0, 0]);
    composite_layer_onto(&mut base, &layer, 2, 2);
    assert_eq!(base, solid(2, 2, [10, 20, 30, 255]));
}

#[test]
fn a_fully_opaque_layer_fully_replaces() {
    let mut base = solid(2, 2, [10, 20, 30, 255]);
    let layer = solid(2, 2, [200, 150, 100, 255]);
    composite_layer_onto(&mut base, &layer, 2, 2);
    assert_eq!(base, solid(2, 2, [200, 150, 100, 255]));
}

#[test]
fn a_half_alpha_layer_blends_to_the_expected_midpoint() {
    let mut base = solid(1, 1, [0, 0, 0, 255]);
    let layer = solid(1, 1, [255, 255, 255, 128]);
    composite_layer_onto(&mut base, &layer, 1, 1);
    // src-over with sa ~= 0.502, da = 1.0 -> out_a = 1.0, out_c = src*sa + dst*(1-sa)
    assert_eq!(base[3], 255);
    assert!(base[0] > 120 && base[0] < 135, "got {}", base[0]);
}
