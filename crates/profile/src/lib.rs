//! Host-side control of a per-profile child process: spawn, liveness
//! check, frame readback, graceful shutdown. The crash-isolation/security
//! boundary the spec calls for — a crashed or hung profile process can't
//! take the shell down with it, and `Drop` force-kills a profile that
//! doesn't respond to `quit()`.
//!
//! No real navigation/input commands yet (see `bin/profile_worker.rs`'s
//! doc comment) - `Profile` only proves the process + IPC transport, not
//! a working browser tab. The command channel is stdin/stdout with a
//! newline-delimited text protocol, not a binary format - simple and
//! good enough at this scale; revisit if the vocabulary grows past a
//! handful of commands.
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

pub use ipc::ShmemError;

/// Total committed memory a single profile process may use before the OS
/// kills it — see `security::sandbox`. 512 MiB comfortably covers a page's
/// DOM/layout/render state at this engine's current scope; revisit once
/// real pages (and their JS heaps) are actually being loaded.
const PROFILE_MEMORY_LIMIT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug)]
pub enum SpawnError {
    Io(std::io::Error),
    Shmem(ShmemError),
    /// Only fatal on platforms where sandboxing is implemented (currently
    /// Windows) — a real failure to confine the process there means the
    /// process would otherwise run unconfined, which this crate refuses to
    /// do silently. On platforms where sandboxing isn't implemented yet
    /// (see `security::sandbox`'s doc comment), `Sandbox(Unsupported)`
    /// never reaches here — `spawn` treats that specific case as an
    /// expected gap, not a failure.
    Sandbox(security::sandbox::SandboxError),
}

impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnError::Io(e) => write!(f, "failed to spawn profile-worker: {e}"),
            SpawnError::Shmem(e) => write!(f, "failed to open profile frame buffer: {e}"),
            SpawnError::Sandbox(e) => write!(f, "failed to sandbox profile-worker: {e}"),
        }
    }
}

impl std::error::Error for SpawnError {}

pub struct Profile {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    frame_reader: ipc::FrameReader,
    quit_sent: bool,
    /// Held only to keep the confinement alive for as long as the process
    /// runs — dropping it (which happens automatically on `Profile`'s own
    /// drop) closes the job, force-killing anything still in it. `None` on
    /// platforms where `security::sandbox::confine` isn't implemented yet;
    /// the existing `Drop`-kill below is the only isolation boundary there.
    _sandbox: Option<security::sandbox::Sandbox>,
}

impl Profile {
    /// Spawns a new `profile-worker` child process rendering into a
    /// `width` × `height` frame published under the shared-memory name
    /// `shmem_name` (must be unique per profile - two profiles sharing a
    /// name would stomp each other's frames).
    ///
    /// `worker_path` is the path to the `profile-worker` executable.
    /// Callers building/testing within this workspace can pass
    /// `env!("CARGO_BIN_EXE_profile-worker")`; a real shell would resolve
    /// this from its own install layout instead.
    pub fn spawn(worker_path: &str, shmem_name: &str, width: u32, height: u32) -> Result<Self, SpawnError> {
        let mut child = Command::new(worker_path)
            .arg(shmem_name)
            .arg(width.to_string())
            .arg(height.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(SpawnError::Io)?;

        let sandbox = match security::sandbox::confine(&child, PROFILE_MEMORY_LIMIT_BYTES) {
            Ok(sandbox) => Some(sandbox),
            Err(e) if cfg!(windows) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(SpawnError::Sandbox(e));
            }
            Err(_unsupported) => None,
        };

        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));

        // The worker creates the shmem region before rendering its first
        // frame, but process startup isn't instant - retry opening our
        // end for a bit rather than racing it once and failing.
        let frame_reader = open_frame_reader_with_retry(shmem_name, width, height)?;

        Ok(Profile {
            child,
            stdin,
            stdout,
            frame_reader,
            quit_sent: false,
            _sandbox: sandbox,
        })
    }

    /// Sends `PING`, waits for `PONG`. `Ok(true)` means the process is
    /// alive and responsive; `Ok(false)` means it responded with
    /// something else (shouldn't happen against a well-behaved worker).
    pub fn ping(&mut self) -> std::io::Result<bool> {
        writeln!(self.stdin, "PING")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        Ok(line.trim() == "PONG")
    }

    /// Asks the worker to re-render and publish a new frame.
    pub fn reload(&mut self) -> std::io::Result<()> {
        writeln!(self.stdin, "RELOAD")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        Ok(())
    }

    /// Asks the worker to fetch `url` (a real HTTP/HTTPS request via
    /// `net::get`, blocking the worker's render loop for the duration —
    /// see `profile-worker`'s doc on why that's an acceptable simplicity
    /// trade at this scope) and render the result, replacing the current
    /// page. `Ok(Ok(()))` means the page loaded; `Ok(Err(message))` means
    /// the worker is still alive and responsive but the fetch/parse
    /// itself failed (bad URL, network error, ...) — the worker renders
    /// a real in-page error message in that case rather than crashing or
    /// leaving the previous page up silently. The outer `io::Result`
    /// only covers the stdin/stdout protocol itself failing (a dead
    /// worker, a broken pipe).
    pub fn navigate(&mut self, url: &str) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "NAVIGATE {url}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// The most recently published frame's raw RGBA8 pixels, or `None` if
    /// the worker hasn't published one yet.
    pub fn latest_frame(&self) -> Option<Vec<u8>> {
        self.frame_reader.latest_frame()
    }

    pub fn frame_generation(&self) -> u32 {
        self.frame_reader.generation()
    }

    /// Sends `QUIT` and waits (up to a short timeout) for the process to
    /// exit on its own. Consumes `self` since there's nothing meaningful
    /// left to do with a `Profile` after this.
    pub fn quit(mut self) {
        self.send_quit_and_wait();
    }

    fn send_quit_and_wait(&mut self) {
        if self.quit_sent {
            return;
        }
        self.quit_sent = true;
        let _ = writeln!(self.stdin, "QUIT");
        let _ = self.stdin.flush();

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // Didn't exit in time - not the crash-isolation boundary's job to
        // wait forever. Drop's kill() covers this if quit() itself wasn't
        // called at all.
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        self.send_quit_and_wait();
        // Force-kill covers both "quit didn't get processed in time" and
        // "Profile was dropped without quit() ever being called" - the
        // crash-isolation guarantee holds either way: a stuck profile
        // process never outlives its Profile handle.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn open_frame_reader_with_retry(name: &str, width: u32, height: u32) -> Result<ipc::FrameReader, SpawnError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last_err = None;
    while Instant::now() < deadline {
        match ipc::FrameReader::new(name, width, height) {
            Ok(reader) => return Ok(reader),
            Err(e) => {
                last_err = Some(e);
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
    Err(SpawnError::Shmem(last_err.unwrap()))
}
