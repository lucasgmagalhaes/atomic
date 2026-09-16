//! Process-level smoke test for the `atomic` binary itself — regression
//! coverage for a real crash observed while manually verifying the
//! automation panel: a first `atomic.exe` launch died within a couple of
//! seconds with no stderr output at all (the process was simply gone on
//! the next check), while a second launch stayed up and responsive. This
//! doesn't reproduce whatever was transient about that (window-manager
//! timing, a race in profile-worker spawn under load, ...), but it does
//! assert the one thing that matters: launched fresh, `atomic.exe` must
//! still be alive a few seconds later, not silently gone.
use std::process::{Child, Command};
use std::time::Duration;

fn kill_and_wait(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn shell_exe_stays_alive_past_startup() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_atomic"))
        .spawn()
        .expect("failed to launch atomic.exe");

    // Real profile-worker spawn + first GPU frame takes a moment - give it
    // more than the couple of seconds the observed crash happened within.
    std::thread::sleep(Duration::from_secs(3));

    match child.try_wait() {
        Ok(None) => {
            // Still running - the expected, healthy outcome.
            kill_and_wait(child);
        }
        Ok(Some(status)) => {
            panic!("atomic.exe exited on its own within 3s (status: {status}) - this is the crash this test guards against");
        }
        Err(e) => {
            kill_and_wait(child);
            panic!("failed to poll atomic.exe's status: {e}");
        }
    }
}
