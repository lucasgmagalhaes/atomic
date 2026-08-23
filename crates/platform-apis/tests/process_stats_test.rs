use platform_apis::process_stats::{cpu_percent, logical_core_count, sample, ProcessStats};
use std::time::Duration;

#[cfg(windows)]
fn spawn_busy_process() -> std::process::Child {
    // Burns real CPU for ~2s so a sample taken mid-loop shows nonzero
    // cpu_time, unlike Start-Sleep (which would show ~0).
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", "$sw = [Diagnostics.Stopwatch]::StartNew(); while ($sw.Elapsed.TotalSeconds -lt 2) { }"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn powershell")
}

#[cfg(windows)]
#[test]
fn sample_reports_real_nonzero_memory_for_a_running_process() {
    let mut child = spawn_busy_process();
    std::thread::sleep(Duration::from_millis(200));

    let stats = sample(child.id()).expect("sample should succeed against a real running process");
    assert!(stats.memory_bytes > 0, "a real process should have nonzero working-set memory");

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
#[test]
fn cpu_time_increases_over_a_real_busy_loop() {
    let mut child = spawn_busy_process();
    std::thread::sleep(Duration::from_millis(100));
    let before = sample(child.id()).expect("first sample should succeed");

    std::thread::sleep(Duration::from_millis(800));
    let after = sample(child.id()).expect("second sample should succeed");

    assert!(after.cpu_time >= before.cpu_time, "cpu_time is cumulative, must never decrease");
    assert!(after.cpu_time > before.cpu_time, "a real busy loop should burn measurable CPU time between samples");

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
#[test]
fn sample_fails_for_a_nonexistent_pid() {
    // PID 0 is the reserved "System Idle Process" - never openable with
    // PROCESS_QUERY_LIMITED_INFORMATION, a stable real failure case
    // without needing to guess at an unused PID.
    let result = sample(0);
    assert!(result.is_err());
}

#[test]
fn cpu_percent_of_a_process_saturating_one_core_is_about_100_over_core_count() {
    let cores = 4;
    let prev = ProcessStats { cpu_time: Duration::from_secs(0), memory_bytes: 0 };
    // Consumed 1 full second of CPU time over 1 second of wall time -
    // fully saturating one core.
    let curr = ProcessStats { cpu_time: Duration::from_secs(1), memory_bytes: 0 };

    let pct = cpu_percent(&prev, &curr, Duration::from_secs(1), cores);
    assert!((pct - 25.0).abs() < 0.01, "expected ~25% (100% / 4 cores), got {pct}");
}

#[test]
fn cpu_percent_is_zero_for_zero_elapsed_wall_time() {
    let prev = ProcessStats { cpu_time: Duration::from_secs(0), memory_bytes: 0 };
    let curr = ProcessStats { cpu_time: Duration::from_secs(1), memory_bytes: 0 };
    assert_eq!(cpu_percent(&prev, &curr, Duration::ZERO, 4), 0.0);
}

#[test]
fn cpu_percent_never_goes_negative_even_if_cpu_time_appears_to_shrink() {
    // Shouldn't happen in practice (cpu_time is cumulative), but a
    // caller feeding samples in the wrong order shouldn't underflow.
    let prev = ProcessStats { cpu_time: Duration::from_secs(5), memory_bytes: 0 };
    let curr = ProcessStats { cpu_time: Duration::from_secs(1), memory_bytes: 0 };
    assert_eq!(cpu_percent(&prev, &curr, Duration::from_secs(1), 4), 0.0);
}

#[test]
fn logical_core_count_is_at_least_one() {
    assert!(logical_core_count() >= 1);
}
