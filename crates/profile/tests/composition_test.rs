//! `COMPOSITION_START`/`COMPOSITION_UPDATE`/`COMPOSITION_END` and
//! `Profile::composition_start`/`composition_update`/`composition_end` —
//! real IME composition dispatch on the focused field. Closes the
//! "IME/composition" part of `spec/matrix/events.md` line 17.

use profile::Profile;

mod common;
use common::*;

#[test]
fn a_full_composition_sequence_commits_text_into_the_focused_field() {
    let page_addr = serve_html_once(
        r##"<input id="field"><style>#field { width: 150px; height: 30px; }</style>
        <script>
          let log = "";
          const field = document.getElementById("field");
          field.addEventListener("compositionstart", e => { log += "start:" + JSON.stringify(e.data) + ","; });
          field.addEventListener("compositionupdate", e => { log += "update:" + e.data + ","; });
          field.addEventListener("compositionend", e => { log += "end:" + e.data + ","; });
          field.addEventListener("input", e => { log += "input:" + e.inputType + ":" + e.data + ","; document.title = log; });
        </script>"##,
    );

    let name = unique_shmem_name("composition-full");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    profile.click_at(10.0, 10.0).unwrap().unwrap();

    let start = profile
        .composition_start()
        .expect("protocol should not fail");
    assert!(start.is_ok(), "composition_start should succeed: {start:?}");

    let update = profile
        .composition_update("ni")
        .expect("protocol should not fail");
    assert!(
        update.is_ok(),
        "composition_update should succeed: {update:?}"
    );

    assert_eq!(
        profile
            .evaluate("document.getElementById('field').value")
            .expect("protocol should not fail")
            .expect("field value should be readable"),
        "",
        "compositionupdate should not commit anything to the field yet"
    );

    let end = profile
        .composition_end("\u{306b}\u{307b}\u{3093}")
        .expect("protocol should not fail");
    assert!(end.is_ok(), "composition_end should succeed: {end:?}");

    assert_eq!(
        profile
            .evaluate("document.getElementById('field').value")
            .expect("protocol should not fail")
            .expect("field value should be readable"),
        "\u{306b}\u{307b}\u{3093}",
        "compositionend should commit the real composed text"
    );

    assert_eq!(
        profile
            .evaluate("document.title")
            .expect("protocol should not fail")
            .expect("title should be readable"),
        "start:\"\",update:ni,end:\u{306b}\u{307b}\u{3093},input:undefined:undefined,",
        "the real event sequence should fire in real IME order"
    );

    profile.quit();
}

#[test]
fn composition_start_without_a_focused_element_reports_an_error() {
    let page_addr = serve_html_once(r##"<input id="field">"##);

    let name = unique_shmem_name("composition-unfocused");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let result = profile
        .composition_start()
        .expect("protocol should not fail");
    assert!(
        result.is_err(),
        "composition_start with nothing focused should report an error"
    );

    profile.quit();
}
