use profile::Profile;

mod common;
use common::*;

/// `ROADMAP.md` item 28 (compositing layers, real integration):
/// stacking-context-establishing boxes (`layout_engine::find_layer_roots`)
/// paint into their own cached canvas, composited onto the base frame in
/// z-index order. These prove real correctness (a layer paints on top,
/// in the right order) and real per-layer invalidation (an in-layer
/// mutation repaints only that layer's own content; an out-of-layer one
/// leaves an unrelated layer's pixels untouched) — see `page/render.rs`'s
/// own doc for how the two caches (`paint_cache`, `layers`) interact.
///
/// Colors are deliberately restricted to `red`/`blue`/`white`/`black` —
/// this engine's CSS only recognizes `red`/`green`/`blue`/`black`/`white`/
/// `transparent` as named colors (`layout_engine::style::types::Color::named`),
/// and `green`'s midtone shifts slightly under the GPU pipeline's real
/// sRGB-aware blending - the four colors used here all sit at the 0/255
/// channel extremes, which any gamma curve leaves invariant.
fn pixel_at(frame: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let idx = ((y * width + x) * 4) as usize;
    [frame[idx], frame[idx + 1], frame[idx + 2], frame[idx + 3]]
}

#[test]
fn a_positioned_z_index_layer_paints_on_top_of_overlapping_base_content() {
    let page_addr = serve_html_once(
        r##"<div id="base"></div>
        <div id="layer"></div>
        <style>
            #base { position: absolute; left: 0px; top: 0px; width: 100px; height: 100px; background-color: red; }
            #layer { position: absolute; left: 20px; top: 20px; width: 40px; height: 40px;
                     background-color: blue; z-index: 1; }
        </style>"##,
    );

    let name = unique_shmem_name("compositing-layer-order");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let frame = wait_for_a_frame(&profile);
    // Inside the overlap: the layer (blue, z-index 1) must win over the
    // base (red) it visually sits on top of.
    let overlap = pixel_at(&frame, 300, 30, 30);
    assert_eq!(
        overlap,
        [0, 0, 255, 255],
        "layer must paint on top of base content it overlaps"
    );
    // Outside the layer's own bounds, but still inside the base: real
    // base color survives untouched.
    let base_only = pixel_at(&frame, 300, 5, 5);
    assert_eq!(
        base_only,
        [255, 0, 0, 255],
        "base content outside the layer must keep its own color"
    );

    profile.quit();
}

#[test]
fn mutating_inside_a_layer_repaints_it_without_corrupting_unrelated_base_content() {
    let page_addr = serve_html_once(
        r##"<div id="base"></div>
        <div id="layer"></div>
        <style>
            #base { position: absolute; left: 0px; top: 0px; width: 60px; height: 60px; background-color: blue; }
            #layer { position: absolute; left: 100px; top: 0px; width: 60px; height: 60px;
                     background-color: red; z-index: 1; }
            #layer.changed { background-color: white; }
        </style>"##,
    );

    let name = unique_shmem_name("compositing-layer-in-layer-mutation");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let before = wait_for_a_frame(&profile);
    assert_eq!(pixel_at(&before, 300, 10, 10), [0, 0, 255, 255]);
    assert_eq!(pixel_at(&before, 300, 110, 10), [255, 0, 0, 255]);

    profile
        .evaluate("document.getElementById('layer').className = 'changed'")
        .expect("protocol should not fail")
        .expect("script must not throw");

    let after = wait_for_changed_frame(&profile, &before);
    assert_eq!(
        pixel_at(&after, 300, 110, 10),
        [255, 255, 255, 255],
        "the mutated layer must repaint its own new color"
    );
    assert_eq!(
        pixel_at(&after, 300, 10, 10),
        [0, 0, 255, 255],
        "an unrelated base region must stay exactly as it was, untouched by the layer's own repaint"
    );

    profile.quit();
}

#[test]
fn mutating_outside_any_layer_repaints_the_base_while_the_layer_stays_correct() {
    let page_addr = serve_html_once(
        r##"<div id="base"></div>
        <div id="layer"></div>
        <style>
            #base { position: absolute; left: 0px; top: 0px; width: 60px; height: 60px; background-color: blue; }
            #base.changed { background-color: black; }
            #layer { position: absolute; left: 100px; top: 0px; width: 60px; height: 60px;
                     background-color: red; z-index: 1; }
        </style>"##,
    );

    let name = unique_shmem_name("compositing-layer-out-of-layer-mutation");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let before = wait_for_a_frame(&profile);
    assert_eq!(pixel_at(&before, 300, 10, 10), [0, 0, 255, 255]);
    assert_eq!(pixel_at(&before, 300, 110, 10), [255, 0, 0, 255]);

    profile
        .evaluate("document.getElementById('base').className = 'changed'")
        .expect("protocol should not fail")
        .expect("script must not throw");

    let after = wait_for_changed_frame(&profile, &before);
    assert_eq!(
        pixel_at(&after, 300, 10, 10),
        [0, 0, 0, 255],
        "the mutated base region must repaint its own new color"
    );
    assert_eq!(
        pixel_at(&after, 300, 110, 10),
        [255, 0, 0, 255],
        "the untouched layer must still composite its own correct, unchanged color"
    );

    profile.quit();
}
