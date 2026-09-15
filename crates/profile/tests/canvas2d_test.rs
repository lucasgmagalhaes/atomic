//! Real `<canvas>` page compositing (`layout_engine::apply_canvas_snapshots`,
//! `js_runtime::canvas_bindings`) — a page's own inline script draws via
//! `getContext('2d')`, and the real GPU-backed pixels should composite
//! into the rendered frame, same as a real fetched `<img>` already does
//! (see `navigate_fetch_test.rs`'s `navigate_fetches_and_paints_a_real_img`
//! for the template this mirrors).

use profile::Profile;

mod common;
use common::*;

#[test]
fn a_canvas_2d_fill_rect_composites_into_the_real_rendered_frame() {
    let html = r#"<canvas id="c" width="300" height="150" style="width:300px;height:150px;"></canvas>
<script>
var ctx = document.getElementById('c').getContext('2d');
ctx.fillStyle = '#ff0000';
ctx.fillRect(0, 0, 300, 150);
</script>"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("canvas2d-fillrect");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let pixels = profile.latest_frame().unwrap();
    let top_left = &pixels[0..4];
    assert_eq!(
        top_left,
        &[255, 0, 0, 255],
        "a real canvas fillRect should composite into the page frame, got {top_left:?}"
    );

    profile.quit();
}

#[test]
fn a_canvas_with_no_getcontext_call_paints_nothing_and_the_page_still_renders() {
    let html = r#"<div id="box" style="width:300px;height:150px;background-color:#00ff00;"></div>
<canvas id="c" width="300" height="150"></canvas>"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("canvas2d-unused");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let pixels = profile.latest_frame().unwrap();
    let top_left = &pixels[0..4];
    assert_eq!(
        top_left,
        &[0, 255, 0, 255],
        "an untouched canvas (no getContext call) shouldn't break the rest of the page's paint, got {top_left:?}"
    );

    profile.quit();
}
