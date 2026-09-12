use profile::Profile;

mod common;
use common::*;

#[test]
fn clicking_to_focus_an_input_updates_its_real_computed_focus_style() {
    // `:focus` changes dom::Dom::style_version, not layout_version - if
    // Page's layout/computed-style cache is keyed on layout_version alone
    // (missing style_version), a click that only focuses an element
    // (nothing layout-affecting) returns the stale pre-focus tree, and
    // this getComputedStyle read would wrongly still see the unfocused
    // backgroundColor.
    let page_addr = serve_html_once(
        r##"<input id="field">
        <style>
            #field { width: 150px; height: 30px; background-color: red; }
            #field:focus { background-color: blue; }
        </style>"##,
    );

    let name = unique_shmem_name("focus-style-cache");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    assert_eq!(
        profile
            .evaluate("getComputedStyle(document.getElementById('field')).backgroundColor")
            .expect("protocol should not fail")
            .expect("computed style should be readable"),
        "rgba(255, 0, 0, 1)",
        "unfocused input should start with its plain background-color"
    );

    let click_result = profile
        .click_at(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        click_result.is_ok(),
        "clicking the real <input> should succeed: {click_result:?}"
    );

    assert_eq!(
        profile
            .evaluate("getComputedStyle(document.getElementById('field')).backgroundColor")
            .expect("protocol should not fail")
            .expect("computed style should be readable"),
        "rgba(0, 0, 255, 1)",
        "focusing the input via a real click must invalidate the cached \
         layout/computed-style tree, not keep serving the stale unfocused one"
    );

    profile.quit();
}

#[test]
fn type_key_with_nothing_focused_reports_an_error() {
    let name = unique_shmem_name("type-key-unfocused");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.type_key("x").expect("protocol should not fail");
    assert!(
        result.is_err(),
        "typing with nothing ever clicked/focused should report an error, not silently no-op"
    );

    profile.quit();
}

#[test]
fn tab_moves_focus_to_the_next_element_in_tab_order() {
    let page_addr = serve_html_once(
        r##"<input id="first"><input id="second"><style>input { width: 60px; height: 20px; }</style>"##,
    );

    let name = unique_shmem_name("tab-moves-focus");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let result = profile.tab(false).expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "tabbing to the first field should succeed: {result:?}"
    );

    assert_eq!(
        profile
            .evaluate("document.activeElement.id")
            .expect("protocol should not fail")
            .expect("active element id should be readable"),
        "first",
        "Tab with nothing focused should move focus to the first tab-order target"
    );

    let result = profile.tab(false).expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "tabbing to the second field should succeed: {result:?}"
    );

    assert_eq!(
        profile
            .evaluate("document.activeElement.id")
            .expect("protocol should not fail")
            .expect("active element id should be readable"),
        "second",
        "a second Tab should move focus to the next tab-order target"
    );

    profile.quit();
}

#[test]
fn a_keydown_listener_that_prevents_default_blocks_tab_focus_movement() {
    let page_addr = serve_html_once(
        r##"<input id="first"><input id="second">
        <style>input { width: 60px; height: 20px; }</style>
        <script>document.addEventListener('keydown', (e) => { if (e.key === 'Tab') e.preventDefault(); });</script>"##,
    );

    let name = unique_shmem_name("tab-prevented");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let result = profile.tab(false).expect("protocol should not fail");
    assert_eq!(
        result,
        Err("default action prevented".to_string()),
        "a keydown listener that calls preventDefault() should block the engine's Tab default action"
    );

    assert_eq!(
        profile
            .evaluate("document.activeElement === null")
            .expect("protocol should not fail")
            .expect("active element check should be readable"),
        "true",
        "focus must not have moved when the keydown default action was prevented"
    );

    profile.quit();
}
