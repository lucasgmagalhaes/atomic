use security::sandbox;

#[cfg(windows)]
fn spawn_long_running_process() -> std::process::Child {
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 30",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn powershell")
}

#[cfg(windows)]
#[test]
fn confine_assigns_the_real_limits_it_asked_for() {
    let mut child = spawn_long_running_process();

    let memory_limit = 256 * 1024 * 1024;
    let sb = sandbox::confine(&child, memory_limit, 1).expect("confine should succeed");

    let (active_process_limit, job_memory_limit) =
        sandbox::query_limits(&sb).expect("query_limits should succeed");
    assert_eq!(active_process_limit, 1);
    assert_eq!(job_memory_limit, memory_limit);

    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
#[test]
fn dropping_the_sandbox_kills_the_confined_process() {
    let child = spawn_long_running_process();
    let pid = child.id();

    let sb = sandbox::confine(&child, 256 * 1024 * 1024, 1).expect("confine should succeed");
    // Deliberately not killing `child` ourselves - only the job's
    // kill-on-close should be what ends it.
    drop(sb);

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut still_alive = true;
    while std::time::Instant::now() < deadline {
        // Checking via `tasklist` rather than `child.try_wait()` is a more
        // independent confirmation the OS actually killed the process, not
        // just that our own `Child` handle thinks so.
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .expect("tasklist should run");
        let listed = String::from_utf8_lossy(&output.stdout);
        if !listed.contains(&pid.to_string()) {
            still_alive = false;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    assert!(
        !still_alive,
        "process should have been killed when the sandbox job was closed"
    );
}

#[cfg(not(windows))]
#[test]
fn confine_is_a_documented_no_op_off_windows() {
    let child = std::process::Command::new("true")
        .spawn()
        .expect("failed to spawn a trivial process");
    let result = sandbox::confine(&child, 1024 * 1024, 1);
    assert!(result.is_err());
}
