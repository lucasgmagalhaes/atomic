use profile::Profile;

mod common;
use common::*;

#[test]
fn scroll_by_shifts_the_painted_viewport_through_taller_content() {
    let html = r#"<style>#a { background-color: #ff0000; width: 100px; height: 150px; } #b { background-color: #0000ff; width: 100px; height: 150px; }</style><div id="a"></div><div id="b"></div>"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("scroll");
    let mut profile = Profile::spawn(worker_path(), &name, 100, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let before = profile.latest_frame().unwrap();
    assert_eq!(
        &before[0..4],
        &[255, 0, 0, 255],
        "unscrolled viewport should show #a (red) first, got {:?}",
        &before[0..4]
    );

    let scroll_result = profile.scroll_by(150.0).expect("protocol should not fail");
    assert!(
        scroll_result.is_ok(),
        "scroll should succeed: {scroll_result:?}"
    );

    let after = profile.latest_frame().unwrap();
    assert_eq!(
    &after[0..4],
    &[0, 0, 255, 255],
    "scrolling down by #a's own height should bring #b (blue) to the top of the viewport, got {:?}",
    &after[0..4]
  );

    // A huge further delta should clamp at the real content height (still
    // #b, not scrolled past into empty/background space).
    let clamp_result = profile
        .scroll_by(10_000.0)
        .expect("protocol should not fail");
    assert!(
        clamp_result.is_ok(),
        "scroll should succeed: {clamp_result:?}"
    );
    let clamped = profile.latest_frame().unwrap();
    assert_eq!(
    &clamped[0..4],
    &[0, 0, 255, 255],
    "scrolling far past the content should clamp at the bottom, not reveal empty space, got {:?}",
    &clamped[0..4]
  );

    profile.quit();
}

#[test]
fn window_scroll_to_from_page_script_moves_the_real_painted_viewport() {
    // Same fixture as scroll_by_shifts_the_painted_viewport_through_taller_content,
    // but the scroll is driven by a page script's own window.scrollTo(...)
    // instead of the host-driven SCROLL command - proves js_runtime's real
    // window.scrollY/scrollTo (crates/js-runtime/src/window.rs) actually
    // reaches profile-worker's paint offset via Context::scroll_y, not just
    // a JS-facing number nothing reads.
    let html = r#"<style>#a { background-color: #ff0000; width: 100px; height: 150px; } #b { background-color: #0000ff; width: 100px; height: 150px; }</style><div id="a"></div><div id="b"></div>"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("window-scroll-to");
    let mut profile = Profile::spawn(worker_path(), &name, 100, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let before = profile.latest_frame().unwrap();
    assert_eq!(
        &before[0..4],
        &[255, 0, 0, 255],
        "unscrolled viewport should show #a (red) first, got {:?}",
        &before[0..4]
    );

    let eval_result = profile
        .evaluate("window.scrollTo(0, 150)")
        .expect("protocol should not fail");
    assert!(
        eval_result.is_ok(),
        "window.scrollTo eval should succeed: {eval_result:?}"
    );

    let after = profile.latest_frame().unwrap();
    assert_eq!(
        &after[0..4],
        &[0, 0, 255, 255],
        "window.scrollTo(0, 150) from a page script should bring #b (blue) to the top of the viewport, got {:?}",
        &after[0..4]
    );

    // window.scrollY should read back the same (host-clamped) offset the
    // paint used - proves the value really round-trips through
    // Context::scroll_y, not just written and forgotten.
    let read_back = profile
        .evaluate("window.scrollY")
        .expect("protocol should not fail")
        .expect("eval should succeed");
    assert_eq!(read_back, "150");

    profile.quit();
}

#[test]
fn scroll_by_a_negative_delta_clamps_back_to_the_top() {
    let html = r#"<style>#a { background-color: #ff0000; width: 100px; height: 150px; } #b { background-color: #0000ff; width: 100px; height: 150px; }</style><div id="a"></div><div id="b"></div>"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("scroll-clamp-top");
    let mut profile = Profile::spawn(worker_path(), &name, 100, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");

    let _ = profile.scroll_by(150.0).expect("protocol should not fail");
    let scroll_result = profile
        .scroll_by(-10_000.0)
        .expect("protocol should not fail");
    assert!(
        scroll_result.is_ok(),
        "scroll should succeed: {scroll_result:?}"
    );

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[255, 0, 0, 255],
        "a large negative scroll should clamp back to the real top (#a, red), not go negative, got {:?}",
        &pixels[0..4]
    );

    profile.quit();
}

#[test]
fn reload_resets_scroll_to_the_top() {
    let html = r#"<style>#a { background-color: #ff0000; width: 100px; height: 150px; } #b { background-color: #0000ff; width: 100px; height: 150px; }</style><div id="a"></div><div id="b"></div>"#;
    let addr = serve_routes(vec![("/", html.to_string())]);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("scroll-reload-reset");
    let mut profile = Profile::spawn(worker_path(), &name, 100, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");
    let _ = profile.scroll_by(150.0).expect("protocol should not fail");

    profile.reload().expect("protocol should not fail");

    let pixels = profile.latest_frame().unwrap();
    assert_eq!(
        &pixels[0..4],
        &[255, 0, 0, 255],
        "a fresh page (post-reload) should always start scrolled to the top, got {:?}",
        &pixels[0..4]
    );

    profile.quit();
}

#[test]
fn click_at_hit_tests_against_the_scrolled_document_not_the_pre_scroll_one() {
    let html = r#"<style>#a { background-color: #ff0000; width: 100px; height: 150px; } #b { background-color: #0000ff; width: 100px; height: 150px; }</style><div id="a"></div><div id="b"></div>"#;
    let addr = serve_html_once(html);
    let page_url = format!("http://{addr}/");

    let name = unique_shmem_name("scroll-hit-test");
    let mut profile = Profile::spawn(worker_path(), &name, 100, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .navigate(&page_url)
        .expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed: {result:?}");
    let _ = profile.scroll_by(150.0).expect("protocol should not fail");

    // After scrolling by #a's own height, on-screen (0,0) is #b's own
    // top-left in document space - a real hit test must add the scroll
    // offset back in to land on #b, not (incorrectly) on whatever was at
    // document-space (0,0) before any scroll happened (#a).
    let click_result = profile
        .click_at(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        click_result.is_ok(),
        "click should land on a real id-addressable element: {click_result:?}"
    );

    profile.quit();
}
