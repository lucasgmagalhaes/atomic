use profile::Profile;

mod common;
use common::*;

/// `ROADMAP.md` P1 item 13 (paint damage tracking): `Page::render` caches
/// its composited pixels keyed the same way `Page::layout`'s own
/// `LayoutCache` is (see that struct's own doc), plus `scroll_top`. These
/// tests prove the cache neither corrupts a static page's output nor goes
/// stale across a real mutation.
#[test]
fn two_renders_of_an_unchanged_page_produce_identical_pixels() {
    let page_addr = serve_html_once(
        r##"<div id="box"></div>
        <style>#box { display: block; width: 80px; height: 40px; background-color: blue; }</style>"##,
    );

    let name = unique_shmem_name("paint-cache-unchanged");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let first = wait_for_a_frame(&profile);
    // Nothing mutates the page in between - the vsync loop keeps calling
    // `render()` every tick regardless, so this proves a cache hit reblits
    // byte-identical pixels rather than corrupting them.
    std::thread::sleep(std::time::Duration::from_millis(100));
    let second = profile
        .latest_frame()
        .expect("worker should still have a published frame");
    assert_eq!(
        first, second,
        "repeated renders of an unchanged page must stay byte-identical"
    );

    profile.quit();
}

#[test]
fn a_real_dom_mutation_invalidates_the_cached_frame() {
    let page_addr = serve_html_once(
        r##"<div id="box"></div>
        <style>
            #box { display: block; width: 80px; height: 40px; background-color: blue; }
            #box.changed { background-color: green; }
        </style>"##,
    );

    let name = unique_shmem_name("paint-cache-dom-mutation");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let before = wait_for_a_frame(&profile);

    profile
        .evaluate("document.getElementById('box').className = 'changed'")
        .expect("protocol should not fail")
        .expect("script must not throw");

    let after = wait_for_changed_frame(&profile, &before);
    assert_ne!(
        before, after,
        "a real style mutation must invalidate the cached frame, not keep serving the stale one"
    );

    profile.quit();
}

#[test]
fn a_focus_only_change_invalidates_the_cached_frame() {
    // `:focus` bumps `dom::Dom::style_version()` without ever bumping
    // `layout_version()` or setting `DirtyFlags::PAINT` (see
    // `PaintCache`'s own doc) - a cache keyed on the wrong signal would
    // silently freeze this exact frame.
    let page_addr = serve_html_once(
        r##"<input id="field">
        <style>
            #field { display: block; width: 150px; height: 30px; background-color: red; }
            #field:focus { background-color: blue; }
        </style>"##,
    );

    let name = unique_shmem_name("paint-cache-focus");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let before = wait_for_a_frame(&profile);

    let click_result = profile
        .click_at(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        click_result.is_ok(),
        "clicking the real <input> should succeed: {click_result:?}"
    );

    let after = wait_for_changed_frame(&profile, &before);
    assert_ne!(
        before, after,
        "focusing the input must repaint its now-blue background, not keep serving the \
         cached pre-focus frame"
    );

    profile.quit();
}
