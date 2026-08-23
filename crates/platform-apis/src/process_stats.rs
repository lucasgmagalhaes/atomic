//! Per-process CPU/memory telemetry — the backend half of the mockup's
//! "Resource monitor (CPU/RAM/FPS per profile, 60s graph, kill process)"
//! gap. This module only does the sampling (real OS calls, not
//! placeholders); the 60s rolling graph, FPS (already available via
//! `profile::Profile::frame_generation()`), and "kill process" (already
//! `profile::Profile::quit()`/`Drop`) are a UI/`apps/shell` concern this
//! module doesn't own.
//!
//! Real per-platform mechanism, same convention as `security::sandbox`:
//! Windows uses `GetProcessTimes` (cumulative kernel+user CPU time) and
//! `GetProcessMemoryInfo` (working-set bytes) via `windows-sys` — real
//! syscalls against a real process handle, tested against an actually
//! spawned process, not a mock. Linux (`/proc/<pid>/stat`+`statm`) and
//! macOS (`task_info`) are different real primitives this crate doesn't
//! implement yet, same "documented gap, not faked" stance `sandbox` takes
//! for those platforms — `sample` returns `Error::Unsupported` there.
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessStats {
    /// Total CPU time (kernel + user) the process has consumed since it
    /// started - a cumulative counter, not a percentage. Compare two
    /// samples' `cpu_time` against the wall-clock time between them (see
    /// [`cpu_percent`]) to get a percentage.
    pub cpu_time: Duration,
    /// Working-set size in bytes (physical memory currently resident -
    /// what Task Manager's "Memory" column shows), not virtual/committed
    /// size.
    pub memory_bytes: u64,
}

#[derive(Debug)]
pub enum Error {
    /// No implementation for this platform yet (see module docs).
    Unsupported,
    /// The OS call itself failed (process exited, access denied, ...) -
    /// carries the raw OS error code for diagnostics.
    Os(u32),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unsupported => write!(f, "process stats are not implemented on this platform"),
            Error::Os(code) => write!(f, "OS call failed (code {code})"),
        }
    }
}

impl std::error::Error for Error {}

/// The number of logical CPUs on this machine - used to normalize
/// [`cpu_percent`] so a single-core-pegged process reads as (roughly)
/// `100 / core_count`, matching how a real resource monitor scales CPU
/// time against wall-clock time across multiple cores.
pub fn logical_core_count() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

/// Percentage of one logical CPU's worth of time `curr` consumed more
/// than `prev` did, over `wall_elapsed` real time, normalized by
/// `core_count` (so a process fully saturating every core reads ~100%,
/// not `100 * core_count`). Returns `0.0` for a zero or negative elapsed
/// duration (can't divide by it) rather than panicking or returning
/// infinity/NaN.
pub fn cpu_percent(prev: &ProcessStats, curr: &ProcessStats, wall_elapsed: Duration, core_count: usize) -> f64 {
    if wall_elapsed.is_zero() || core_count == 0 {
        return 0.0;
    }
    let cpu_delta = curr.cpu_time.saturating_sub(prev.cpu_time);
    let raw = cpu_delta.as_secs_f64() / wall_elapsed.as_secs_f64() * 100.0;
    raw / core_count as f64
}

#[cfg(windows)]
pub fn sample(pid: u32) -> Result<ProcessStats, Error> {
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME};
    use windows_sys::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ};

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return Err(Error::Os(windows_sys::Win32::Foundation::GetLastError()));
        }

        let (mut creation, mut exit, mut kernel, mut user) = (
            std::mem::zeroed::<FILETIME>(),
            std::mem::zeroed::<FILETIME>(),
            std::mem::zeroed::<FILETIME>(),
            std::mem::zeroed::<FILETIME>(),
        );
        let times_ok = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) != 0;
        if !times_ok {
            let err = windows_sys::Win32::Foundation::GetLastError();
            CloseHandle(handle);
            return Err(Error::Os(err));
        }

        let mut counters = std::mem::zeroed::<PROCESS_MEMORY_COUNTERS>();
        counters.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let mem_ok = GetProcessMemoryInfo(handle, &mut counters, counters.cb) != 0;
        let mem_err = if !mem_ok { Some(windows_sys::Win32::Foundation::GetLastError()) } else { None };

        CloseHandle(handle);

        if let Some(err) = mem_err {
            return Err(Error::Os(err));
        }

        Ok(ProcessStats {
            cpu_time: filetime_to_duration(kernel) + filetime_to_duration(user),
            memory_bytes: counters.WorkingSetSize as u64,
        })
    }
}

#[cfg(windows)]
fn filetime_to_duration(ft: windows_sys::Win32::Foundation::FILETIME) -> Duration {
    // FILETIME is 100-nanosecond intervals, split across two u32s.
    let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    Duration::from_nanos(ticks * 100)
}

#[cfg(not(windows))]
pub fn sample(_pid: u32) -> Result<ProcessStats, Error> {
    Err(Error::Unsupported)
}
