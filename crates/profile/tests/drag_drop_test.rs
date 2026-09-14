//! `DRAG_START`/`DROP_AT` and `Profile::drag_start`/`drop_at` — real
//! coordinate-driven `dragstart`/`dragover`/`drop`/`dragend` dispatch.
//! Closes the "drag-and-drop" part of `spec/matrix/events.md` line 17.

use profile::Profile;

mod common;
use common::*;

const TWO_TARGETS_HTML: &str = r##"<div id="source">drag me</div>
        <div id="target">drop here</div>
        <style>
          #source { width: 100px; height: 40px; }
          #target { width: 100px; height: 40px; }
        </style>"##;

#[test]
fn a_full_drag_and_drop_sequence_carries_real_data_to_the_target() {
    let page_addr = serve_html_once(&format!(
        "{TWO_TARGETS_HTML}\n<script>\n\
          let log = '';\n\
          document.getElementById('source').addEventListener('dragstart', e => {{ e.dataTransfer.setData('text/plain', 'payload'); log += 'dragstart,'; }});\n\
          document.getElementById('source').addEventListener('dragend', () => {{ log += 'dragend,'; document.title = log; }});\n\
          document.getElementById('target').addEventListener('dragover', e => {{ e.preventDefault(); log += 'dragover,'; }});\n\
          document.getElementById('target').addEventListener('drop', e => {{ log += 'drop:' + e.dataTransfer.getData('text/plain') + ','; }});\n\
        </script>"
    ));

    let name = unique_shmem_name("drag-drop-full");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let start = profile
        .drag_start(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(start.is_ok(), "drag_start should succeed: {start:?}");

    let drop = profile
        .drop_at(10.0, 60.0)
        .expect("protocol should not fail");
    assert!(drop.is_ok(), "drop_at should succeed: {drop:?}");

    assert_eq!(
        profile
            .evaluate("document.title")
            .expect("protocol should not fail")
            .expect("title should be readable"),
        "dragstart,dragover,drop:payload,dragend,",
        "the real drag sequence should fire in real DOM order carrying real data"
    );

    profile.quit();
}

#[test]
fn drop_without_a_preceding_drag_start_reports_an_error() {
    let page_addr = serve_html_once(TWO_TARGETS_HTML);

    let name = unique_shmem_name("drag-drop-no-start");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let result = profile
        .drop_at(10.0, 60.0)
        .expect("protocol should not fail");
    assert!(
        result.is_err(),
        "drop_at with no drag in progress should report an error"
    );

    profile.quit();
}

#[test]
fn a_target_that_never_calls_preventdefault_rejects_the_drop() {
    let page_addr = serve_html_once(&format!(
        "{TWO_TARGETS_HTML}\n<script>\n\
          let dropped = false;\n\
          document.getElementById('target').addEventListener('drop', () => {{ dropped = true; }});\n\
        </script>"
    ));

    let name = unique_shmem_name("drag-drop-rejected");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    profile
        .drag_start(10.0, 10.0)
        .expect("protocol should not fail")
        .expect("drag_start should succeed");

    let result = profile
        .drop_at(10.0, 60.0)
        .expect("protocol should not fail");
    assert!(
        result.is_err(),
        "a target that never calls preventDefault on dragover should reject the drop"
    );

    assert_eq!(
        profile
            .evaluate("dropped")
            .expect("protocol should not fail")
            .expect("dropped should be readable"),
        "false",
        "the real drop event should never have fired"
    );

    profile.quit();
}
