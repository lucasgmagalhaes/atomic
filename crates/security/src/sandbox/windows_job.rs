//! Real Windows sandbox: a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
//! (closing our handle force-kills every process still assigned, even if
//! `profile::Profile`'s own `Drop`-kill somehow doesn't run), a hard cap on
//! how many processes may ever be active in the job, and a hard cap on the
//! job's total committed memory — genuine OS-enforced limits, not just
//! crash-isolation bookkeeping.
use std::os::windows::io::AsRawHandle;
use std::process::Child;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

#[derive(Debug)]
pub enum SandboxError {
    CreateJobObject(std::io::Error),
    SetInformation(std::io::Error),
    AssignProcess(std::io::Error),
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::CreateJobObject(e) => write!(f, "CreateJobObjectW failed: {e}"),
            SandboxError::SetInformation(e) => write!(f, "SetInformationJobObject failed: {e}"),
            SandboxError::AssignProcess(e) => write!(f, "AssignProcessToJobObject failed: {e}"),
        }
    }
}

impl std::error::Error for SandboxError {}

/// Owns the Job Object handle. Every process assigned to it is force-killed
/// when this drops (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`), so keep it alive
/// for exactly as long as the confined process should be allowed to run.
pub struct Sandbox {
    job: HANDLE,
}

// The HANDLE is a plain kernel-object handle, not thread-affine (unlike a
// GUI window handle) - Win32 job objects are explicitly safe to touch from
// any thread, so there's no soundness issue sending this across one.
unsafe impl Send for Sandbox {}

impl Drop for Sandbox {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.job);
        }
    }
}

/// Assigns `child` to a fresh Job Object capped at `memory_limit_bytes` of
/// total committed memory and `1` active process (a profile worker isn't
/// expected to spawn grandchildren; if it ever needs to, this limit moves
/// with it). Returns the [`Sandbox`] guard - drop it to tear down the
/// confinement (and kill anything still running in it).
pub fn confine(child: &Child, memory_limit_bytes: u64) -> Result<Sandbox, SandboxError> {
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(SandboxError::CreateJobObject(std::io::Error::last_os_error()));
    }
    let sandbox = Sandbox { job };

    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_JOB_MEMORY;
    info.BasicLimitInformation.ActiveProcessLimit = 1;
    info.JobMemoryLimit = memory_limit_bytes as usize;

    let ok = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 {
        return Err(SandboxError::SetInformation(std::io::Error::last_os_error()));
    }

    let process_handle = child.as_raw_handle() as HANDLE;
    let ok = unsafe { AssignProcessToJobObject(job, process_handle) };
    if ok == 0 {
        return Err(SandboxError::AssignProcess(std::io::Error::last_os_error()));
    }

    Ok(sandbox)
}

/// Reads back the limits currently set on `sandbox`'s job — `(active_process_limit, job_memory_limit_bytes)`.
/// Test-only round-trip: proves `confine` actually programmed the OS, not
/// just that the calls returned success codes.
pub fn query_limits(sandbox: &Sandbox) -> Result<(u32, u64), SandboxError> {
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    let mut returned: u32 = 0;
    let ok = unsafe {
        QueryInformationJobObject(
            sandbox.job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            &mut returned,
        )
    };
    if ok == 0 {
        return Err(SandboxError::SetInformation(std::io::Error::last_os_error()));
    }
    Ok((
        info.BasicLimitInformation.ActiveProcessLimit as u32,
        info.JobMemoryLimit as u64,
    ))
}
