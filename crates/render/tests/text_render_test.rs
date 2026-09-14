use css::parse_stylesheet;
use dom::Dom;
use layout_engine::{build_box_tree, layout_block};
use render::{build_glyph_list, composite_glyphs, ClipRect, ClippedGlyph};

#[test]
fn rendering_text_paints_non_background_pixels() {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("A");
    d.append_child(root, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("p { color: #ff0000; font-size: 64px; }");
    let mut tree = build_box_tree(&d, p, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let glyphs = build_glyph_list(&tree);
    assert!(!glyphs.is_empty());

    let width = 200u32;
    let height = 100u32;
    let mut pixels = vec![0u8; (width * height * 4) as usize]; // transparent black
    composite_glyphs(&mut pixels, width, height, &glyphs);

    // At least one pixel should now be a shade of red with some alpha -
    // exact glyph shape isn't asserted (font hinting/AA make that
    // fragile), just that *something* got painted and it's red-ish, not
    // still fully transparent.
    let painted_red = pixels
        .chunks_exact(4)
        .any(|px| px[0] > 0 && px[3] > 0 && px[1] == 0 && px[2] == 0);
    assert!(
        painted_red,
        "expected at least one red, non-transparent pixel from rendering 'A'"
    );
}

#[test]
fn empty_text_paints_nothing() {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    d.append_child(root, p);

    let sheet = parse_stylesheet("");
    let mut tree = build_box_tree(&d, p, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let glyphs = build_glyph_list(&tree);
    assert!(glyphs.is_empty());

    let mut pixels = vec![0u8; 4 * 4 * 4];
    composite_glyphs(&mut pixels, 4, 4, &glyphs);
    assert!(pixels.iter().all(|&b| b == 0));
}

#[test]
fn glyphs_outside_the_buffer_are_clipped_without_panicking() {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("hello");
    d.append_child(root, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("p { font-size: 200px; }"); // huge, likely overflows a tiny buffer
    let mut tree = build_box_tree(&d, p, &sheet).unwrap();
    layout_block(&mut tree, 2000.0, 0.0, 0.0);

    let glyphs = build_glyph_list(&tree);
    let mut pixels = vec![0u8; 4 * 4 * 4];
    // Must not panic on out-of-bounds writes.
    composite_glyphs(&mut pixels, 4, 4, &glyphs);
}

#[test]
fn clip_restricts_which_pixels_a_glyph_can_paint() {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("A");
    d.append_child(root, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("p { color: #ff0000; font-size: 64px; }");
    let mut tree = build_box_tree(&d, p, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let glyphs = build_glyph_list(&tree);
    assert!(!glyphs.is_empty());

    let width = 200u32;
    let height = 100u32;

    // Sanity: unclipped, this glyph does paint something (matches
    // `rendering_text_paints_non_background_pixels` above).
    let mut unclipped_pixels = vec![0u8; (width * height * 4) as usize];
    composite_glyphs(&mut unclipped_pixels, width, height, &glyphs);
    assert!(unclipped_pixels.iter().any(|&b| b > 0));

    // Same glyphs, wrapped in a clip region far from where "A" actually
    // paints (near the origin) - real per-pixel clipping means nothing
    // should land inside that far corner.
    let clipped: Vec<ClippedGlyph> = glyphs
        .iter()
        .map(|g| ClippedGlyph {
            glyph: g.glyph,
            clip: Some(ClipRect {
                x: 150.0,
                y: 50.0,
                width: 50.0,
                height: 50.0,
            }),
            opacity: 1.0,
            fixed: false,
            sticky: None,
        })
        .collect();

    let mut clipped_pixels = vec![0u8; (width * height * 4) as usize];
    composite_glyphs(&mut clipped_pixels, width, height, &clipped);
    assert!(
        clipped_pixels.iter().all(|&b| b == 0),
        "clip region doesn't overlap the glyph - nothing should paint"
    );
}

#[test]
fn zero_opacity_paints_no_glyph_pixels() {
    let mut d = Dom::new();
    let root = d.root();
    let p = d.create_element("p");
    let text = d.create_text("A");
    d.append_child(root, p);
    d.append_child(p, text);

    let sheet = parse_stylesheet("p { color: #ff0000; font-size: 64px; }");
    let mut tree = build_box_tree(&d, p, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let glyphs = build_glyph_list(&tree);
    assert!(!glyphs.is_empty());

    let transparent: Vec<ClippedGlyph> = glyphs
        .iter()
        .map(|g| ClippedGlyph {
            glyph: g.glyph,
            clip: None,
            opacity: 0.0,
            fixed: false,
            sticky: None,
        })
        .collect();

    let width = 200u32;
    let height = 100u32;
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    composite_glyphs(&mut pixels, width, height, &transparent);
    assert!(
        pixels.iter().all(|&b| b == 0),
        "opacity 0 should paint nothing"
    );
}
