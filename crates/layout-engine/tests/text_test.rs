use layout_engine::{layout_text, Color};

const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };

#[test]
fn empty_text_has_zero_size_and_no_glyphs() {
    let layout = layout_text("", 16.0, None, BLACK);
    assert_eq!(layout.width, 0.0);
    assert_eq!(layout.height, 0.0);
    assert!(layout.glyphs.is_empty());
}

#[test]
fn measures_a_single_unwrapped_line() {
    let layout = layout_text("hello", 16.0, None, BLACK);
    assert!(layout.width > 0.0, "width should be positive, got {}", layout.width);
    assert!(layout.height > 0.0);
    assert_eq!(layout.glyphs.len(), 5, "one glyph per character in a simple ASCII word");
}

#[test]
fn longer_text_measures_wider_than_shorter_text_at_the_same_size() {
    let short = layout_text("hi", 16.0, None, BLACK);
    let long = layout_text("hello world", 16.0, None, BLACK);
    assert!(long.width > short.width);
}

#[test]
fn larger_font_size_measures_wider_and_taller() {
    let small = layout_text("hello", 12.0, None, BLACK);
    let large = layout_text("hello", 32.0, None, BLACK);
    assert!(large.width > small.width);
    assert!(large.height > small.height);
}

#[test]
fn wraps_onto_multiple_lines_when_narrower_than_the_text() {
    let unwrapped = layout_text("hello world this is a long sentence", 16.0, None, BLACK);
    let wrapped = layout_text("hello world this is a long sentence", 16.0, Some(80.0), BLACK);

    // Wrapping should never make a line wider than the constraint...
    assert!(wrapped.width <= 80.0 + 1.0); // +1 for float slop
    // ...and forcing multiple lines should make the total height taller
    // than a single unwrapped line.
    assert!(wrapped.height > unwrapped.height);
}

#[test]
fn glyphs_are_positioned_left_to_right() {
    let layout = layout_text("ab", 16.0, None, BLACK);
    assert_eq!(layout.glyphs.len(), 2);
    assert!(layout.glyphs[1].x > layout.glyphs[0].x);
}

#[test]
fn glyph_color_matches_the_requested_color() {
    let red = Color { r: 255, g: 0, b: 0, a: 255 };
    let layout = layout_text("x", 16.0, None, red);
    assert_eq!(layout.glyphs[0].color, red);
}
