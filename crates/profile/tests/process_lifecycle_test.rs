use profile::Profile;

mod common;
use common::*;

#[test]
fn spawns_a_real_process_and_responds_to_ping() {
    let name = unique_shmem_name("ping");
    let mut profile = Profile::spawn(worker_path(), &name, 64, 64).expect("spawn should succeed");

    assert!(profile.ping().unwrap(), "worker should reply PONG to PING");

    profile.quit();
}

#[test]
fn publishes_a_real_rendered_frame_on_startup() {
    let name = unique_shmem_name("frame");
    let profile = Profile::spawn(worker_path(), &name, 64, 64).expect("spawn should succeed");

    // The worker renders and publishes before entering its command loop,
    // but process startup + first GPU init isn't instant.
    let pixels = wait_for_a_frame(&profile);

    assert_eq!(pixels.len(), 64 * 64 * 4);
    // The demo page has a dark, non-black background (#1a1c2b) - at least
    // confirm the frame isn't just the shmem region's zeroed initial
    // state (which would read as fully transparent black).
    let non_zero = pixels.chunks_exact(4).any(|px| px != [0, 0, 0, 0]);
    assert!(non_zero, "rendered frame should not be all-zero");

    profile.quit();
}

#[test]
fn reload_publishes_a_new_generation() {
    let name = unique_shmem_name("reload");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");

    let gen0 = wait_for_generation_after(&profile, 0);

    profile.reload().unwrap();
    assert!(profile.frame_generation() > gen0);

    profile.quit();
}

#[test]
fn resize_republishes_a_frame_at_the_new_dimensions_and_flips_a_height_media_query() {
    let name = unique_shmem_name("resize");
    let mut profile = Profile::spawn(worker_path(), &name, 100, 100).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let html = "<style>#box{background-color:red}@media (min-height: 300px){#box{background-color:blue}}</style><div id=\"box\">x</div>";
    let addr = serve_html_once(html);
    profile
        .navigate(&format!("http://{addr}/"))
        .unwrap()
        .unwrap();
    wait_for_a_frame(&profile);

    // Below the media query's threshold at the original 100px height - the
    // unconditional `background-color: red` rule applies.
    let before = profile
        .evaluate(
            "getComputedStyle(document.getElementById('box')).getPropertyValue('background-color')",
        )
        .unwrap()
        .unwrap();
    assert_eq!(before, "rgba(255, 0, 0, 1)");

    profile.resize(100, 400).unwrap().unwrap();
    // The resized worker republishes into a *new* shared-memory segment
    // (see `Profile::resize`'s own doc), whose generation counter starts
    // fresh at 0 - comparing it against the old segment's generation
    // wouldn't mean anything, so this only checks that the new segment has
    // a real published frame at all.
    assert!(profile.frame_generation() > 0);

    let frame = profile
        .latest_frame()
        .expect("resize should publish a frame");
    assert_eq!(frame.len(), 100 * 400 * 4);

    // Above the threshold now - the media query flips, and a real
    // synchronous `"resize"` event already ran on `window` by the time
    // this evaluates (see `Context::fire_resize`).
    let after = profile
        .evaluate(
            "getComputedStyle(document.getElementById('box')).getPropertyValue('background-color')",
        )
        .unwrap()
        .unwrap();
    assert_eq!(after, "rgba(0, 0, 255, 1)");

    profile.quit();
}

#[test]
fn quit_makes_the_child_process_exit() {
    let name = unique_shmem_name("quit");
    let profile = Profile::spawn(worker_path(), &name, 16, 16).expect("spawn should succeed");
    // quit() consumes self and waits (with a timeout) for real exit -
    // if this hangs, the test framework's own timeout will catch it.
    profile.quit();
}

#[test]
fn frame_generation_advances_on_its_own_without_any_reload() {
    let name = unique_shmem_name("vsync-gen");
    let profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let gen0 = profile.frame_generation();
    let gen1 = wait_for_generation_after(&profile, gen0 + 3);

    // ~60fps over 200ms should easily clear a handful of new frames -
    // this is the vsync loop's whole point: frames keep publishing without
    // any RELOAD (or any other command) being sent at all.
    assert!(
        gen1 > gen0 + 3,
        "frame generation should keep advancing on its own, got {gen0} -> {gen1}"
    );

    profile.quit();
}

#[test]
fn set_fps_cap_actually_slows_down_the_render_loops_own_cadence() {
    let name = unique_shmem_name("fps-cap");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .set_fps_cap(5)
        .expect("protocol should not fail against a live worker");
    assert!(
        result.is_ok(),
        "a positive fps cap should be accepted: {result:?}"
    );

    let gen0 = profile.frame_generation();
    std::thread::sleep(std::time::Duration::from_millis(250));
    let gen1 = profile.frame_generation();

    // At 5fps, 250ms should produce roughly one new frame, nowhere near the
    // ~15 a real ~60fps loop would - proves SET_FPS_CAP actually reached
    // the render loop's own scheduling, not just returned success.
    let advanced = gen1 - gen0;
    assert!(
    advanced <= 4,
    "capped loop should advance only a couple frames in 250ms, got {advanced} ({gen0} -> {gen1})"
  );

    profile.quit();
}

#[test]
fn set_fps_cap_rejects_a_non_positive_value() {
    let name = unique_shmem_name("fps-cap-invalid");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile
        .set_fps_cap(0)
        .expect("protocol should not fail against a live worker");
    assert!(
        result.is_err(),
        "a zero fps cap should be rejected, not silently accepted"
    );

    profile.quit();
}

#[test]
fn pause_actually_stops_frame_generation_and_resume_restarts_it() {
    let name = unique_shmem_name("pause");
    let mut profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    profile.pause().expect("pause should reach a live worker");
    let gen_paused_start = profile.frame_generation();
    std::thread::sleep(std::time::Duration::from_millis(100));
    let gen_paused_end = profile.frame_generation();
    assert_eq!(
    gen_paused_start, gen_paused_end,
    "a paused loop must not publish new frames at all, got {gen_paused_start} -> {gen_paused_end}"
  );

    // Still responsive to PING while paused - not a hung/dead process.
    assert!(profile
        .ping()
        .expect("ping should still reach a paused worker"));

    profile.resume().expect("resume should reach a live worker");
    let gen_resumed_start = profile.frame_generation();
    let gen_resumed_end = wait_for_generation_after(&profile, gen_resumed_start);
    assert!(
    gen_resumed_end > gen_resumed_start,
    "resuming should restart real frame generation, got {gen_resumed_start} -> {gen_resumed_end}"
  );

    profile.quit();
}

#[test]
fn dropping_without_quit_still_kills_the_process() {
    let name = unique_shmem_name("drop");
    {
        let _profile = Profile::spawn(worker_path(), &name, 16, 16).expect("spawn should succeed");
        // Dropped here without calling quit() - Drop must force-kill it
        // rather than leaking a live child process.
    }
    // No direct assertion possible without a process handle (consumed by
    // Drop) - this test's value is Drop not panicking/hanging, and it
    // documents the guarantee. Process leaks would only surface in a
    // real leak-detection harness, not this test.
}
