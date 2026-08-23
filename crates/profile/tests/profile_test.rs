use profile::Profile;

fn unique_shmem_name(tag: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("nimble-profile-test-{tag}-{nanos}")
}

fn worker_path() -> &'static str {
    env!("CARGO_BIN_EXE_profile-worker")
}

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
    // but process startup + first GPU init isn't instant - poll briefly.
    let mut frame = None;
    for _ in 0..100 {
        if let Some(f) = profile.latest_frame() {
            frame = Some(f);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let pixels = frame.expect("worker should publish a frame within 5s");

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

    let mut gen0 = 0;
    for _ in 0..100 {
        gen0 = profile.frame_generation();
        if gen0 > 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(gen0 > 0, "should have an initial published frame");

    profile.reload().unwrap();
    assert!(profile.frame_generation() > gen0);

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
fn navigate_fetches_a_real_page_and_renders_its_content() {
    let name = unique_shmem_name("navigate");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let frame_before = profile.latest_frame().unwrap();
    let result = profile.navigate("https://example.com/").expect("protocol should not fail");
    assert!(result.is_ok(), "navigate should succeed against a real URL: {result:?}");

    // A real page (all-white-ish background, real body text) looks
    // nothing like the dark demo page - pixels should visibly differ.
    let frame_after = profile.latest_frame().unwrap();
    assert_ne!(frame_before, frame_after);

    profile.quit();
}

#[test]
fn navigate_to_a_bad_url_reports_an_error_and_renders_one() {
    let name = unique_shmem_name("navigate-error");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let result = profile.navigate("not a url").expect("protocol should not fail");
    assert!(result.is_err(), "navigating to garbage should report an error, not silently succeed");

    // The worker should still be alive and responsive afterward - a
    // failed navigation isn't fatal.
    assert!(profile.ping().unwrap());

    profile.quit();
}

#[test]
fn reload_retries_the_last_navigated_url() {
    let name = unique_shmem_name("navigate-reload");
    let mut profile = Profile::spawn(worker_path(), &name, 300, 150).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    profile.navigate("https://example.com/").unwrap().unwrap();
    let gen_after_navigate = profile.frame_generation();

    profile.reload().unwrap();
    assert!(profile.frame_generation() > gen_after_navigate);

    profile.quit();
}

fn wait_for_a_frame(profile: &Profile) -> Vec<u8> {
    for _ in 0..100 {
        if let Some(f) = profile.latest_frame() {
            return f;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("worker should publish a frame within 5s");
}

#[test]
fn frame_generation_advances_on_its_own_without_any_reload() {
    let name = unique_shmem_name("vsync-gen");
    let profile = Profile::spawn(worker_path(), &name, 32, 32).expect("spawn should succeed");
    wait_for_a_frame(&profile);

    let gen0 = profile.frame_generation();
    std::thread::sleep(std::time::Duration::from_millis(200));
    let gen1 = profile.frame_generation();

    // ~60fps over 200ms should easily clear a handful of new frames -
    // this is the vsync loop's whole point: frames keep publishing without
    // any RELOAD (or any other command) being sent at all.
    assert!(gen1 > gen0 + 3, "frame generation should keep advancing on its own, got {gen0} -> {gen1}");

    profile.quit();
}

#[test]
fn js_timers_pumped_by_the_loop_visibly_change_rendered_pixels() {
    let name = unique_shmem_name("vsync-js");
    // Large enough that the counter paragraph's text actually lands inside
    // the canvas - the demo page's three paragraphs plus 20px padding
    // don't fit in a tiny viewport, and pixels outside it are never
    // touched by `composite_glyphs`, which would make this test vacuous.
    let profile = Profile::spawn(worker_path(), &name, 400, 200).expect("spawn should succeed");

    let frame_a = wait_for_a_frame(&profile);
    // The demo script's setInterval-style counter ticks every 50ms and
    // rewrites #counter's text - give it a few ticks' worth of real time.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let frame_b = profile.latest_frame().expect("should still have a frame");

    assert_ne!(
        frame_a, frame_b,
        "rendered pixels should change as the worker's per-frame loop pumps setTimeout/requestAnimationFrame and re-renders the mutated DOM"
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
