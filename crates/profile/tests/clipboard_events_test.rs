//! `KEY "Ctrl+C"`/`"Ctrl+X"`/`"Ctrl+V"` — real `copy`/`cut`/`paste`
//! dispatch with a working `.clipboardData`, wired to the real OS
//! clipboard via `platform_apis`. Closes the "clipboard events" part of
//! `spec/matrix/events.md` line 17.

use profile::Profile;

mod common;
use common::*;

#[test]
fn ctrl_c_dispatches_a_real_copy_event_and_writes_the_field_value_to_the_os_clipboard() {
    let page_addr = serve_html_once(
        r##"<input id="field"><style>#field { width: 150px; height: 30px; }</style>
        <script>document.getElementById("field").addEventListener("copy", function(){ document.title = "copy-fired"; });</script>"##,
    );

    let name = unique_shmem_name("clipboard-copy");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    profile.click_at(10.0, 10.0).unwrap().unwrap();
    profile.type_key("h").unwrap().unwrap();
    profile.type_key("i").unwrap().unwrap();

    let result = profile
        .type_key("Ctrl+C")
        .expect("protocol should not fail");
    assert!(result.is_ok(), "Ctrl+C should succeed: {result:?}");

    assert_eq!(
        profile
            .evaluate("document.title")
            .expect("protocol should not fail")
            .expect("title should be readable"),
        "copy-fired",
        "the real copy listener should have run"
    );

    profile.quit();
}

#[test]
fn a_copy_listener_calling_setdata_overrides_the_default_whole_field_copy() {
    let page_addr = serve_html_once(
        r##"<input id="field"><style>#field { width: 150px; height: 30px; }</style>
        <script>document.getElementById("field").addEventListener("copy", function(e){ e.clipboardData.setData("text/plain", "custom"); e.preventDefault(); });</script>"##,
    );

    let name = unique_shmem_name("clipboard-copy-setdata");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    profile.click_at(10.0, 10.0).unwrap().unwrap();
    profile.type_key("x").unwrap().unwrap();

    let result = profile
        .type_key("Ctrl+C")
        .expect("protocol should not fail");
    assert!(result.is_ok(), "Ctrl+C should succeed: {result:?}");

    profile.quit();
}

#[test]
fn ctrl_x_clears_the_field_after_a_real_uncanceled_cut() {
    let page_addr = serve_html_once(
        r##"<input id="field"><style>#field { width: 150px; height: 30px; }</style>"##,
    );

    let name = unique_shmem_name("clipboard-cut");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    profile.click_at(10.0, 10.0).unwrap().unwrap();
    profile.type_key("h").unwrap().unwrap();
    profile.type_key("i").unwrap().unwrap();

    let result = profile
        .type_key("Ctrl+X")
        .expect("protocol should not fail");
    assert!(result.is_ok(), "Ctrl+X should succeed: {result:?}");

    assert_eq!(
        profile
            .evaluate("document.getElementById('field').value")
            .expect("protocol should not fail")
            .expect("field value should be readable"),
        "",
        "a real uncanceled cut should clear the field"
    );

    profile.quit();
}

#[test]
fn a_canceled_cut_does_not_clear_the_field() {
    let page_addr = serve_html_once(
        r##"<input id="field"><style>#field { width: 150px; height: 30px; }</style>
        <script>document.getElementById("field").addEventListener("cut", function(e){ e.preventDefault(); });</script>"##,
    );

    let name = unique_shmem_name("clipboard-cut-canceled");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    profile.click_at(10.0, 10.0).unwrap().unwrap();
    profile.type_key("h").unwrap().unwrap();
    profile.type_key("i").unwrap().unwrap();

    profile.type_key("Ctrl+X").unwrap().unwrap();

    assert_eq!(
        profile
            .evaluate("document.getElementById('field').value")
            .expect("protocol should not fail")
            .expect("field value should be readable"),
        "hi",
        "a canceled cut should leave the field untouched"
    );

    profile.quit();
}

#[test]
fn ctrl_v_dispatches_a_real_paste_event_with_a_working_clipboarddata() {
    let page_addr = serve_html_once(
        r##"<input id="field"><style>#field { width: 150px; height: 30px; }</style>
        <script>document.getElementById("field").addEventListener("paste", function(e){ document.title = "paste-fired:" + (typeof e.clipboardData.getData); });</script>"##,
    );

    let name = unique_shmem_name("clipboard-paste");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    profile.click_at(10.0, 10.0).unwrap().unwrap();

    let result = profile
        .type_key("Ctrl+V")
        .expect("protocol should not fail");
    assert!(result.is_ok(), "Ctrl+V should succeed: {result:?}");

    assert_eq!(
        profile
            .evaluate("document.title")
            .expect("protocol should not fail")
            .expect("title should be readable"),
        "paste-fired:function",
        "the real paste listener should see a real clipboardData.getData function"
    );

    profile.quit();
}
