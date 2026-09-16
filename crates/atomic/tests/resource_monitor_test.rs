//! Real OS process sampling, not mocked: every test here samples this
//! test binary's own running process (`std::process::id()`), the same way
//! `platform_apis::process_stats`'s own tests sample a really-spawned
//! process elsewhere in this workspace.
use atomic::resource_monitor::PaneMonitor;
use std::time::Duration;

#[test]
fn tick_accumulates_real_cpu_memory_and_fps_after_the_sample_interval() {
    let pid = std::process::id();
    let mut monitor = PaneMonitor::new();

    // First tick always samples (see PaneMonitor::new's doc: it seeds
    // `last_sampled_at` in the past) but has no prior sample to diff a
    // CPU delta against yet.
    monitor.tick(pid, 0);
    assert!(
        monitor.latest_memory_bytes().unwrap() > 0,
        "a real process should report non-zero working-set memory"
    );
    assert!(monitor.latest_cpu_percent().is_none());
    assert!(monitor.latest_fps().is_none());

    std::thread::sleep(Duration::from_millis(1100));
    // Burn real CPU so the delta the next sample sees isn't pure noise.
    let mut x: u64 = 0;
    for i in 0..50_000_000u64 {
        x = x.wrapping_add(i);
    }
    std::hint::black_box(x);

    monitor.tick(pid, 60);
    let cpu = monitor
        .latest_cpu_percent()
        .expect("second tick has a prior sample to diff against");
    assert!(cpu >= 0.0);
    assert_eq!(monitor.cpu_history().count(), 1);

    let fps = monitor
        .latest_fps()
        .expect("frame_generation advanced by 60 over ~1.1s");
    assert!(fps > 0.0, "got {fps}");
}

#[test]
fn tick_before_the_sample_interval_elapses_is_a_no_op() {
    let pid = std::process::id();
    let mut monitor = PaneMonitor::new();

    monitor.tick(pid, 0);
    let memory_after_first = monitor.latest_memory_bytes();

    // Called again immediately - well under the 1s sample interval.
    monitor.tick(pid, 999);
    assert_eq!(
        monitor.latest_fps(),
        None,
        "frame_generation shouldn't register as advanced - the second tick was throttled away"
    );
    assert_eq!(monitor.latest_memory_bytes(), memory_after_first);
    assert_eq!(monitor.cpu_history().count(), 0);
}
