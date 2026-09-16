//! Per-platform confinement for a spawned `profile-worker` child process —
//! the spec's "process sandboxing" row (Windows AppContainer/Job Objects,
//! Linux seccomp+namespaces, macOS App Sandbox: three separate
//! platform-specific mechanisms, deliberately not unified behind one
//! lowest-common-denominator API beyond `confine`'s signature).
//!
//! Only Windows is implemented, via Job Objects — real OS-enforced limits
//! (kill-on-job-close, an active-process cap, a total-memory cap), tested
//! against an actual spawned process on this dev machine. Linux
//! (`unshare`/`clone` into fresh PID/mount/network namespaces plus a
//! seccomp-bpf syscall filter) and macOS (`sandbox_init`'s SBPL profiles —
//! deprecated but still the only public API — or the App Sandbox
//! entitlement model) are real, different primitives that need a
//! Linux/macOS machine to write and test against; not implemented here.
//! `confine` is a documented no-op returning `Err(Unsupported)` on every
//! non-Windows target rather than silently pretending to sandbox.
#[cfg(windows)]
mod windows_job;
#[cfg(windows)]
pub use windows_job::{confine, query_limits, Sandbox, SandboxError};

#[cfg(not(windows))]
mod unsupported {
    use std::process::Child;

    #[derive(Debug)]
    pub enum SandboxError {
        Unsupported,
    }

    impl std::fmt::Display for SandboxError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "process sandboxing is not implemented on this platform yet"
            )
        }
    }

    impl std::error::Error for SandboxError {}

    /// Zero-sized: nothing to hold on a platform where `confine` never
    /// succeeds.
    pub struct Sandbox;

    pub fn confine(
        _child: &Child,
        _memory_limit_bytes: u64,
        _active_process_limit: u32,
    ) -> Result<Sandbox, SandboxError> {
        Err(SandboxError::Unsupported)
    }
}
#[cfg(not(windows))]
pub use unsupported::{confine, Sandbox, SandboxError};
