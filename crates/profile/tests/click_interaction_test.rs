use profile::Profile;

mod common;
use common::*;

#[test]
fn click_at_a_real_coordinate_dispatches_a_real_click_and_changes_the_page() {
    // #target sits at the document's real top-left flow position (no
    // position/float exists in this engine - see mockup/rendering-engine-gaps.md -
    // so it's exactly where a first block-level child with no margin
    // always lands) and gets a real click listener that mutates its own
    // textContent, giving a real, observable pixel change on success.
    // <style>/<script> come *after* #target - this engine has no UA
    // stylesheet hiding <head>/<style> (see mockup/rendering-engine-gaps.md),
    // so raw CSS/JS source text placed before an element in the markup
    // renders as real visible text and pushes later content down; placing
    // it after keeps #target at its real, predictable (0,0) flow position
    // (matches the convention this file's own existing style-block test
    // already uses).
    let page_addr = serve_html_once(
        r##"<div id="target">click me</div>
        <style>#target { width: 100px; height: 40px; }</style>
        <script>document.getElementById("target").addEventListener("click", function(){ document.getElementById("target").textContent = "clicked!"; });</script>"##,
    );

    let name = unique_shmem_name("click-at");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let frame_before = profile.latest_frame().unwrap();
    let result = profile
        .click_at(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "a click on the real target element should succeed: {result:?}"
    );

    let frame_after = profile.latest_frame().unwrap();
    assert_ne!(
        frame_before, frame_after,
        "the click listener's real textContent mutation should visibly change rendered pixels"
    );

    profile.quit();
}

#[test]
fn loaded_page_selectors_and_bubbling_events_flow_through_profile_worker() {
    let page_addr = serve_html_once(
        r##"<div id="parent"><button id="child" class="action">before</button></div>
        <style>#child { width: 100px; height: 40px; }</style>
        <script>
          const child = document.querySelector('.action');
          const parent = document.querySelector('#parent');
          if (child === null || document.querySelectorAll('.action').length !== 1) throw new Error('selector failure');
          parent.addEventListener('click', event => {
            if (event.target === child && event.currentTarget === parent && event.type === 'click') parent.textContent = 'bubbled';
          });
          child.addEventListener('click', event => { if (event.currentTarget === child) child.textContent = 'target'; });
        </script>"##,
    );

    let name = unique_shmem_name("selector-event-worker");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let frame_before = profile.latest_frame().unwrap();
    let result = profile
        .click_at(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "the selected child should receive the native click: {result:?}"
    );
    assert_ne!(
        frame_before,
        profile.latest_frame().unwrap(),
        "the parent bubbling listener should visibly update the loaded page"
    );

    profile.quit();
}

#[test]
fn loaded_page_can_append_to_document_body_and_repaint_after_a_click() {
    // The button is visible at the top-left before its script runs. Its
    // listener appends a new node through `document.body`, proving that a
    // mutation made by page JS is picked up by the command-path repaint,
    // rather than only mutations of nodes that existed during page load.
    let page_addr = serve_html_once(
        r##"<button id="add">add</button>
        <style>#add { width: 100px; height: 40px; }</style>
        <script>
          document.getElementById('add').addEventListener('click', () => {
            const notice = document.createElement('p');
            notice.textContent = 'added after load';
            document.body.appendChild(notice);
          });
        </script>"##,
    );

    let name = unique_shmem_name("dynamic-body-worker");
    // This engine still lays out inline script source as text, so the
    // appended paragraph lands after that source. Keep the viewport tall
    // enough to observe it in this end-to-end rendering assertion.
    let mut profile = Profile::spawn(worker_path(), &name, 300, 700).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let frame_before = profile.latest_frame().unwrap();
    let result = profile
        .click_at(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "the button should receive the native click: {result:?}"
    );
    assert_ne!(
        frame_before,
        profile.latest_frame().unwrap(),
        "a node appended through document.body should be included in the command-path repaint"
    );

    profile.quit();
}

#[test]
fn click_at_a_point_with_no_element_reports_an_error() {
    // A real, deliberately tiny page - block layout means nothing here
    // extends anywhere near the bottom-right of a 300x150 frame, so that
    // corner is genuinely outside every real box, not just "probably".
    let page_addr = serve_html_once(r##"<div id="tiny">x</div>"##);

    let name = unique_shmem_name("click-at-empty");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let result = profile
        .click_at(299.0, 149.0)
        .expect("protocol should not fail");
    assert!(
        result.is_err(),
        "a click with no real element underneath should report an error, not silently succeed"
    );

    profile.quit();
}

#[test]
fn get_bounding_client_rect_reflects_the_real_rendered_layout() {
    // #box starts blue (.narrow) sized 100x50 by real CSS. Its own click
    // handler measures itself via a real getBoundingClientRect() and only
    // swaps to red (.wide) if the measured width/height match what the
    // real layout engine actually computed - proving the rect Page::render
    // pushed via Context::set_layout_rects (after the *first* real render,
    // from navigate) is genuinely visible to a script running later, not
    // just to js-runtime's own isolated unit tests.
    let html = r#"<div id="box" class="narrow"></div>
        <style>.narrow { background-color: #0000ff; width: 100px; height: 50px; } .wide { background-color: #ff0000; width: 100px; height: 50px; }</style>
        <script>document.getElementById('box').addEventListener('click', () => {
            const r = document.getElementById('box').getBoundingClientRect();
            if (r.width === 100 && r.height === 50) {
                document.getElementById('box').className = 'wide';
            }
        });</script>"#;
    let page_addr = serve_html_once(html);
    let page_url = format!("http://{page_addr}/");

    let name = unique_shmem_name("bounding-rect");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    profile.navigate(&page_url).unwrap().unwrap();
    let before = profile.latest_frame().unwrap();
    assert_eq!(
        &before[0..4],
        &[0, 0, 255, 255],
        "should start blue (.narrow)"
    );

    let click_result = profile
        .click_at(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        click_result.is_ok(),
        "clicking the real box should succeed: {click_result:?}"
    );

    let after = profile.latest_frame().unwrap();
    assert_eq!(
        &after[0..4],
        &[255, 0, 0, 255],
        "the click handler's real getBoundingClientRect() measurement should have matched, swapping to .wide (red)"
    );

    profile.quit();
}

#[test]
fn click_at_then_type_key_types_into_a_real_focused_input() {
    let page_addr = serve_html_once(
        r##"<input id="field"><style>#field { width: 150px; height: 30px; }</style>"##,
    );

    let name = unique_shmem_name("type-key");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let click_result = profile
        .click_at(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        click_result.is_ok(),
        "clicking the real <input> should succeed: {click_result:?}"
    );

    let type_result = profile.type_key("h").expect("protocol should not fail");
    assert!(
        type_result.is_ok(),
        "typing into the real focused input should succeed: {type_result:?}"
    );

    assert_eq!(
        profile
            .evaluate("document.getElementById('field').value")
            .expect("protocol should not fail")
            .expect("field value should be readable"),
        "h",
        "typing a real character should update the focused input's value"
    );

    profile.quit();
}
