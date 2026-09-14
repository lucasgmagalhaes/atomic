//! `CONTEXT_MENU_AT`/`Profile::context_menu_at` — real coordinate-driven
//! right-click: dispatches a real bubbling, cancelable `"contextmenu"`
//! `MouseEvent` on the nearest id-addressable ancestor. Closes the
//! "context menu" part of `spec/matrix/events.md` line 17.

use profile::Profile;

mod common;
use common::*;

#[test]
fn right_clicking_a_real_element_dispatches_a_real_contextmenu() {
    let page_addr = serve_html_once(
        r##"<div id="target">right click me</div>
        <style>#target { width: 100px; height: 40px; }</style>
        <script>document.getElementById("target").addEventListener("contextmenu", function(e){ e.preventDefault(); document.getElementById("target").textContent = "menu:" + e.button; });</script>"##,
    );

    let name = unique_shmem_name("context-menu");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let frame_before = profile.latest_frame().unwrap();
    let result = profile
        .context_menu_at(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "right-clicking the real target element should succeed: {result:?}"
    );

    assert_eq!(
        profile
            .evaluate("document.getElementById('target').textContent")
            .expect("protocol should not fail")
            .expect("textContent should be readable"),
        "menu:2",
        "the real contextmenu listener should see button === 2"
    );
    assert_ne!(
        frame_before,
        profile.latest_frame().unwrap(),
        "the real textContent mutation should visibly change rendered pixels"
    );

    profile.quit();
}

#[test]
fn right_clicking_a_point_with_no_element_reports_an_error() {
    let page_addr = serve_html_once(r##"<div id="tiny">x</div>"##);

    let name = unique_shmem_name("context-menu-empty");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let result = profile
        .context_menu_at(299.0, 149.0)
        .expect("protocol should not fail");
    assert!(
        result.is_err(),
        "a right-click with no real element underneath should report an error"
    );

    profile.quit();
}
