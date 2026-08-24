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
        Self::spawn_with_proxy(worker_path, shmem_name, width, height, None)
    }

    /// Same as [`spawn`](Self::spawn), plus a per-profile upstream HTTP/
    /// HTTPS proxy every `NAVIGATE`/`RELOAD` fetch (page HTML, `<link>`
    /// stylesheets, `@import`s) is routed through instead of connecting
    /// directly — closes the spec's "proxy por perfil" requirement (`net`
    /// itself already tunnels via a real `CONNECT`; this is what actually
    /// picks a proxy *for this profile*). `proxy` is `"host:port"` or
    /// `"user:pass@host:port"` (matching a `user:pass@host:port` proxy
    /// URL's authority, minus the scheme, which this crate doesn't
    /// interpret — it's forwarded to the worker as-is and parsed there).
    /// `None` behaves exactly like `spawn` (connects directly, no proxy) —
    /// the worker's own argument parsing treats a missing 5th argument the
    /// same as this crate not passing one, so the two paths converge on
    /// the exact same child-process invocation rather than one being a
    /// degraded version of the other.
    pub fn spawn_with_proxy(worker_path: &str, shmem_name: &str, width: u32, height: u32, proxy: Option<&str>) -> Result<Self, SpawnError> {
        Self::spawn_with_proxy_and_dns(worker_path, shmem_name, width, height, proxy, None)
    }

    /// Same as [`spawn`](Self::spawn), plus a per-profile custom DNS
    /// server (`"host:port"`, the server's own IP:port — this doesn't
    /// resolve a hostname for *that*) every `NAVIGATE`/`RELOAD` fetch
    /// resolves its target host through instead of the OS resolver, via
    /// `net::get_via_dns` — closes the "DNS" half of the spec's Settings/
    /// Network requirement the same way [`spawn_with_proxy`](Self::spawn_with_proxy)
    /// closed the proxy half. `None` behaves exactly like [`spawn`](Self::spawn).
    pub fn spawn_with_dns(worker_path: &str, shmem_name: &str, width: u32, height: u32, dns_server: Option<&str>) -> Result<Self, SpawnError> {
        Self::spawn_with_proxy_and_dns(worker_path, shmem_name, width, height, None, dns_server)
    }

    /// Same as [`spawn`](Self::spawn), plus opening a specific GPU adapter
    /// (see `render::list_adapters`'s own doc for how a caller enumerates
    /// real ones) instead of `render::GpuRenderer::new`'s default-adapter
    /// heuristic — the mockup's "Settings > Performance > GPU" knob.
    /// `index` is into `render::list_adapters()`'s own order; an
    /// out-of-range index is the worker process's problem to report (it
    /// panics on that, same as `GpuRenderer::new_with_adapter` itself
    /// does), not this crate's.
    pub fn spawn_with_gpu_adapter(worker_path: &str, shmem_name: &str, width: u32, height: u32, gpu_adapter: Option<usize>) -> Result<Self, SpawnError> {
        Self::spawn_full(worker_path, shmem_name, width, height, None, None, gpu_adapter)
    }

    /// The general form every other `spawn_*` delegates to. A proxy and a
    /// custom DNS server can both be passed, but the worker's own
    /// `fetch_with_cookies` always prefers the proxy when both are set
    /// (there's no `net` entry point combining `CONNECT` tunneling with a
    /// caller-chosen resolver — a proxied request's DNS resolution is the
    /// proxy's job). `gpu_adapter` is independent of both — it picks which
    /// GPU renders, not how network requests are routed.
    pub fn spawn_with_proxy_and_dns(
        worker_path: &str,
        shmem_name: &str,
        width: u32,
        height: u32,
        proxy: Option<&str>,
        dns_server: Option<&str>,
    ) -> Result<Self, SpawnError> {
        Self::spawn_full(worker_path, shmem_name, width, height, proxy, dns_server, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn spawn_full(
        worker_path: &str,
        shmem_name: &str,
        width: u32,
        height: u32,
        proxy: Option<&str>,
        dns_server: Option<&str>,
        gpu_adapter: Option<usize>,
    ) -> Result<Self, SpawnError> {
        let mut command = Command::new(worker_path);
        command.arg(shmem_name).arg(width.to_string()).arg(height.to_string());
        if proxy.is_some() || dns_server.is_some() || gpu_adapter.is_some() {
            command.arg(proxy.unwrap_or(""));
        }
        if dns_server.is_some() || gpu_adapter.is_some() {
            command.arg(dns_server.map(str::to_string).unwrap_or_default());
        }
        if let Some(gpu_adapter) = gpu_adapter {
            command.arg(gpu_adapter.to_string());
        }
        let mut child = command
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

    /// The OS process id of the spawned `profile-worker` child - real
    /// value from `std::process::Child::id()`, not synthesized. Lets a
    /// caller sample real per-process telemetry against it (e.g.
    /// `platform_apis::process_stats::sample(profile.pid())` for the
    /// mockup's resource-monitor CPU/RAM column) without this crate having
    /// to depend on `platform-apis` itself.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Dispatches a real `"click"` event at the element with id `selector`
    /// (only `#id` is accepted — see `profile-worker`'s own doc on why).
    /// `Ok(Ok(()))` means it dispatched (a page-attached `"click"` listener,
    /// if any, actually ran); `Ok(Err(message))` means the worker is still
    /// alive but the click itself failed (no such id, or `selector` wasn't
    /// `#id`-shaped). The outer `io::Result` only covers the protocol
    /// itself failing (a dead worker, a broken pipe) — same convention as
    /// [`navigate`](Self::navigate).
    pub fn click(&mut self, selector: &str) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "CLICK {selector}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Sets the `#id` element's `textContent` to `value` — see
    /// `profile-worker`'s own doc on why this is a deviation from a real
    /// `HTMLInputElement.value` assignment (this engine has no such
    /// property), not an equivalent of one. `value` must not contain a
    /// newline (this crate's stdin/stdout protocol is newline-delimited —
    /// see this struct's own doc comment); a value that does never reaches
    /// the worker, reported the same way a worker-side failure would be
    /// rather than corrupting the command stream.
    pub fn fill(&mut self, selector: &str, value: &str) -> std::io::Result<Result<(), String>> {
        if value.contains('\n') {
            return Ok(Err("fill value must not contain a newline".to_string()));
        }
        writeln!(self.stdin, "FILL {selector} {value}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Caps the worker's own vsync render loop at `fps` frames per second
    /// (the mockup's "Settings > Performance > frame cap" knob) - takes
    /// effect on the worker's very next tick, not a respawn (unlike the
    /// proxy/DNS settings, which are fixed at spawn time - the render loop
    /// itself has no equivalent "set once" constraint, so this is real live
    /// throttling of an already-running profile). `Ok(Err(message))` if
    /// `fps` isn't a positive integer the worker accepted.
    pub fn set_fps_cap(&mut self, fps: u32) -> std::io::Result<Result<(), String>> {
        writeln!(self.stdin, "SET_FPS_CAP {fps}")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        let line = line.trim();
        match line.strip_prefix("ERROR ") {
            Some(message) => Ok(Err(message.to_string())),
            None => Ok(Ok(())),
        }
    }

    /// Pauses the worker's own vsync render loop entirely (no JS timer
    /// pumping, no re-render, no frame publish - genuinely idle, see
    /// `profile-worker`'s own `paused` doc) - the mockup's "background
    /// throttling" knob for a pane that isn't currently visible. `PING`/
    /// `SET_FPS_CAP`/`QUIT` still work while paused. No failure mode
    /// beyond the protocol itself, unlike `click`/`fill`/`navigate` -
    /// there's nothing about "pause" that can be rejected.
    pub fn pause(&mut self) -> std::io::Result<()> {
        writeln!(self.stdin, "PAUSE")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        Ok(())
    }

    /// Resumes a paused profile's render loop - a no-op (still answers
    /// `RESUMED`) if it wasn't paused.
    pub fn resume(&mut self) -> std::io::Result<()> {
        writeln!(self.stdin, "RESUME")?;
        self.stdin.flush()?;
        let mut line = String::new();
        self.stdout.read_line(&mut line)?;
        Ok(())
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
