//! `MOUSE_MOVE`/`Profile::mouse_move` — real coordinate-driven hover:
//! `dom::Dom`'s `:hover` state plus real bubbling `mouseover`/`mouseout`
//! and non-bubbling `mouseenter`/`mouseleave` dispatch. Closes the
//! "hover/enter/leave" part of `spec/matrix/events.md` line 17.

use profile::Profile;

mod common;
use common::*;

#[test]
fn moving_onto_a_real_element_dispatches_a_real_mouseover() {
    let page_addr = serve_html_once(
        r##"<div id="target">hover me</div>
        <style>#target { width: 100px; height: 40px; }</style>
        <script>document.getElementById("target").addEventListener("mouseover", function(){ document.getElementById("target").textContent = "hovered!"; });</script>"##,
    );

    let name = unique_shmem_name("mouse-move-over");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let frame_before = profile.latest_frame().unwrap();
    let result = profile
        .mouse_move(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "moving onto the real target element should succeed: {result:?}"
    );

    let frame_after = profile.latest_frame().unwrap();
    assert_ne!(
        frame_before, frame_after,
        "the mouseover listener's real textContent mutation should visibly change rendered pixels"
    );

    profile.quit();
}

#[test]
fn moving_off_an_element_dispatches_a_real_mouseout() {
    let page_addr = serve_html_once(
        r##"<div id="target">hover me</div>
        <style>#target { width: 100px; height: 40px; }</style>
        <script>document.getElementById("target").addEventListener("mouseout", function(){ document.getElementById("target").textContent = "left!"; });</script>"##,
    );

    let name = unique_shmem_name("mouse-move-out");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    profile
        .mouse_move(10.0, 10.0)
        .expect("protocol should not fail")
        .expect("moving onto the real target should succeed");
    let frame_hovered = profile.latest_frame().unwrap();

    let result = profile
        .mouse_move(299.0, 149.0)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "moving off the real target to an empty point should still succeed: {result:?}"
    );

    let frame_after = profile.latest_frame().unwrap();
    assert_ne!(
        frame_hovered, frame_after,
        "the mouseout listener's real textContent mutation should visibly change rendered pixels"
    );

    profile.quit();
}

#[test]
fn moving_within_the_same_element_does_not_redispatch() {
    let page_addr = serve_html_once(
        r##"<div id="target">hover me</div>
        <style>#target { width: 100px; height: 40px; }</style>
        <script>
          let count = 0;
          document.getElementById("target").addEventListener("mouseover", function(){
            count += 1;
            document.getElementById("target").textContent = "count:" + count;
          });
        </script>"##,
    );

    let name = unique_shmem_name("mouse-move-same");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    profile
        .mouse_move(10.0, 10.0)
        .expect("protocol should not fail")
        .expect("first move onto the target should succeed");
    let frame_after_first = profile.latest_frame().unwrap();

    profile
        .mouse_move(20.0, 15.0)
        .expect("protocol should not fail")
        .expect("second move within the same target should succeed");
    let frame_after_second = profile.latest_frame().unwrap();

    assert_eq!(
        frame_after_first, frame_after_second,
        "moving within the same real element should not re-dispatch mouseover"
    );

    profile.quit();
}

#[test]
fn moving_onto_a_nested_element_dispatches_mouseenter_on_every_id_addressable_ancestor() {
    let page_addr = serve_html_once(
        r##"<div id="outer"><div id="inner">hover me</div></div>
        <style>#outer { width: 100px; height: 60px; } #inner { width: 80px; height: 40px; }</style>
        <script>
          let log = "";
          document.getElementById("outer").addEventListener("mouseenter", () => { log += "outer,"; });
          document.getElementById("inner").addEventListener("mouseenter", () => {
            log += "inner,";
            document.getElementById("inner").textContent = log;
          });
        </script>"##,
    );

    let name = unique_shmem_name("mouse-enter-nested");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    let frame_before = profile.latest_frame().unwrap();
    let result = profile
        .mouse_move(10.0, 10.0)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "moving onto the nested real target should succeed: {result:?}"
    );

    assert_eq!(
        profile
            .evaluate("document.getElementById('inner').textContent")
            .expect("protocol should not fail")
            .expect("textContent should be readable"),
        "inner,",
        "mouseenter should fire on the inner element before the outer listener's log check runs"
    );
    assert_ne!(
        frame_before,
        profile.latest_frame().unwrap(),
        "the real textContent mutation should visibly change rendered pixels"
    );

    profile.quit();
}

#[test]
fn moving_off_a_nested_element_to_an_unrelated_one_dispatches_mouseleave_on_every_stale_ancestor() {
    let page_addr = serve_html_once(
        r##"<div id="outer"><div id="inner">hover me</div></div>
        <div id="other">elsewhere</div>
        <style>
          #outer { width: 100px; height: 60px; }
          #inner { width: 80px; height: 40px; }
          #other { width: 100px; height: 20px; }
        </style>
        <script>
          let log = "";
          document.getElementById("inner").addEventListener("mouseleave", () => { log += "inner,"; });
          document.getElementById("outer").addEventListener("mouseleave", () => {
            log += "outer,";
            document.getElementById("other").textContent = log;
          });
        </script>"##,
    );

    let name = unique_shmem_name("mouse-leave-nested");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);
    profile
        .navigate(&format!("http://{page_addr}/"))
        .expect("protocol should not fail")
        .expect("navigate should succeed");

    profile
        .mouse_move(10.0, 10.0)
        .expect("protocol should not fail")
        .expect("moving onto the nested real target should succeed");

    let result = profile
        .mouse_move(10.0, 70.0)
        .expect("protocol should not fail");
    assert!(
        result.is_ok(),
        "moving onto the unrelated real target should succeed: {result:?}"
    );

    assert_eq!(
        profile
            .evaluate("document.getElementById('other').textContent")
            .expect("protocol should not fail")
            .expect("textContent should be readable"),
        "inner,outer,",
        "mouseleave should fire on the inner element (innermost first) before the outer ancestor"
    );

    profile.quit();
}
