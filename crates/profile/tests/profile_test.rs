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
