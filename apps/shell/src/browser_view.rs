//! Wires the GUI to the real render stack: spawns a `profile::Profile`
//! (a genuine child `profile-worker` process, running its own real
//! per-frame vsync loop and publishing frames over shared memory - see
//! `profile`'s and `profile-worker`'s own docs), polls its latest
//! published frame every repaint, and uploads changed frames as an egui
//! texture. No GPU work happens in this crate - the frame bytes are
//! already fully rendered RGBA8 by the time they reach here; this is
//! purely "display what the profile process already drew."
//!
//! Real address-bar navigation too: [`BrowserView::navigate`] sends a
//! `NAVIGATE <url>` command to the worker (`profile::Profile::navigate`),
//! which does a real `net::get` fetch and renders whatever comes back -
//! see `profile-worker`'s own doc for what that does and doesn't cover
//! (no per-page stylesheet extraction, no redirects). A failed navigation
//! surfaces as [`BrowserView::navigation_error`] without losing the
//! address bar's text or crashing the view.
//!
//! Scoped down from a real browser chrome: one profile, no tabs/workspace
//! *switcher* UI yet (`main.rs` now uses `workspace::WorkspaceManager` to
//! name the one running profile for `automation`'s `pane(...)` — see
//! `NimbleApp::run_automation_script` — but there's still no UI to create/
//! switch workspaces or spawn more than one profile into one), a fixed
//! frame size decided at spawn time (a real implementation would re-spawn
//! - or resize the shared-memory region - on window resize; this doesn't).
//!
//! [`BrowserView::spawn_with_proxy`] routes the spawned profile's fetches
//! through a real upstream proxy (`profile::Profile::spawn_with_proxy` →
//! `net::get_via_proxy`, a genuine `CONNECT` tunnel - see that crate's own
//! docs) - a proxy is fixed for a profile's process lifetime, so changing
//! it means spawning a fresh `BrowserView`, not mutating a running one.
use std::path::PathBuf;

/// Locates the `profile-worker` binary that should already exist next to
/// this executable (a real workspace build via `cargo build --workspace`
/// puts every binary target in the same output directory). Also checks
/// one directory up, since `cargo test` binaries run from
/// `target/debug/deps/` while `profile-worker` itself lands in
/// `target/debug/` - a real installed build only ever needs the first
/// check; the second exists purely so this function is testable without
/// a separate install step.
pub fn worker_binary_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let name = if cfg!(windows) { "profile-worker.exe" } else { "profile-worker" };
    let dir = exe.parent().ok_or_else(|| "executable has no parent directory".to_string())?;

    for candidate_dir in [dir, dir.parent().unwrap_or(dir)] {
        let candidate = candidate_dir.join(name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(format!("profile-worker binary ({name}) not found next to {}", exe.display()))
}

/// Converts a tightly-packed RGBA8 frame (`profile::Profile::latest_frame`'s
/// own format) into an `egui::ColorImage` ready to upload as a texture.
pub fn rgba_to_color_image(pixels: &[u8], width: u32, height: u32) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied([width as usize, height as usize], pixels)
}

/// Owns one live profile process and the texture built from its most
/// recently seen frame.
pub struct BrowserView {
    profile: Option<profile::Profile>,
    texture: Option<egui::TextureHandle>,
    last_generation: u32,
    width: u32,
    height: u32,
    error: Option<String>,
    /// What the address bar should show - starts empty (the built-in demo
    /// page, not a real URL), set to whatever was last passed to
    /// [`navigate`](Self::navigate) regardless of whether it succeeded
    /// (matches `profile-worker`'s own "RELOAD retries the last attempted
    /// URL, including a failed one" semantics).
    current_url: String,
    /// Set by a failed [`navigate`](Self::navigate) call, cleared by the
    /// next successful one. Independent of `error` (a spawn/process-level
    /// failure) - this is specifically "the page didn't load."
    navigation_error: Option<String>,
}

impl BrowserView {
    /// Spawns a fresh profile at `width` x `height` with no proxy - see
    /// [`spawn_with_proxy`](Self::spawn_with_proxy).
    pub fn spawn(width: u32, height: u32) -> Self {
        Self::spawn_with_proxy(width, height, None)
    }

    /// Same as [`spawn`](Self::spawn), routing every fetch the spawned
    /// profile makes through `proxy` (`"host:port"` or
    /// `"user:pass@host:port"` - see `profile::Profile::spawn_with_proxy`
    /// and `profile-worker`'s `parse_proxy_arg` for the exact grammar and
    /// what an unparseable value degrades to). Failure (missing binary,
    /// shared-memory setup failure, ...) is stored rather than propagated
    /// - the GUI shows it inline instead of failing to start.
    ///
    /// A profile's proxy is fixed for its process's lifetime (matches
    /// `profile-worker`'s own "set once at spawn" scope) - changing it
    /// means spawning a new `BrowserView`, which is why this constructor
    /// exists separately from a settable field on an existing one; the
    /// shared-memory name includes a counter so a shell that respawns more
    /// than once per process (e.g. to change the proxy) never collides
    /// with the region a still-shutting-down previous profile might still
    /// hold.
    pub fn spawn_with_proxy(width: u32, height: u32, proxy: Option<&str>) -> Self {
        static SPAWN_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SPAWN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let shmem_name = format!("nimble-shell-{}-{n}", std::process::id());
        Self::spawn_with_shmem_name(&shmem_name, width, height, proxy)
    }

    /// Same as [`spawn_with_proxy`](Self::spawn_with_proxy), but keyed to a
    /// caller-chosen stable `pane_id` instead of a per-process spawn
    /// counter - `profile-worker`'s `storage_root` is derived directly from
    /// the shared-memory name (see that binary's `main`), so a stable name
    /// here is what makes a pane's `document.cookie`/`localStorage`/
    /// `indexedDB` actually survive a shell restart instead of starting
    /// fresh every launch (closes CLAUDE.md's "doesn't persist across
    /// separate profile-worker process launches" storage gap, for the
    /// `apps/shell` caller specifically). Two panes with the same `pane_id`
    /// across two concurrently-running shell processes would collide on
    /// the same shared-memory region and storage directory - not handled
    /// here, same as this project's existing "no per-profile identity
    /// beyond a GUI-assigned id" scope.
    pub fn spawn_with_identity(pane_id: &str, width: u32, height: u32, proxy: Option<&str>) -> Self {
        let shmem_name = format!("nimble-profile-{pane_id}");
        Self::spawn_with_shmem_name(&shmem_name, width, height, proxy)
    }

    fn spawn_with_shmem_name(shmem_name: &str, width: u32, height: u32, proxy: Option<&str>) -> Self {
        let (profile, error) = match worker_binary_path() {
            Ok(path) => match profile::Profile::spawn_with_proxy(&path.to_string_lossy(), shmem_name, width, height, proxy) {
                Ok(p) => (Some(p), None),
                Err(e) => (None, Some(e.to_string())),
            },
            Err(e) => (None, Some(e)),
        };

        BrowserView { profile, texture: None, last_generation: 0, width, height, error, current_url: String::new(), navigation_error: None }
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn navigation_error(&self) -> Option<&str> {
        self.navigation_error.as_deref()
    }

    pub fn current_url(&self) -> &str {
        &self.current_url
    }

    /// Lends the live profile process to a caller that needs to drive it
    /// directly — e.g. `automation::AutomationEngine`, which borrows
    /// (rather than owns) the panes it controls precisely so a profile can
    /// still be displayed while a script also drives it. `None` if this
    /// view failed to spawn (see [`error`](Self::error)).
    pub fn profile_mut(&mut self) -> Option<&mut profile::Profile> {
        self.profile.as_mut()
    }

    /// The live profile process's OS pid, for real per-process telemetry
    /// (see `platform_apis::process_stats`) - `None` if this view failed
    /// to spawn.
    pub fn pid(&self) -> Option<u32> {
        self.profile.as_ref().map(|p| p.pid())
    }

    /// The live profile process's current frame generation (see
    /// `profile::Profile::frame_generation`) - `None` if this view failed
    /// to spawn. For a resource monitor deriving FPS from how fast this
    /// counter advances, not for texture-upload logic (`poll_texture`
    /// already tracks its own generation internally).
    pub fn frame_generation(&self) -> Option<u32> {
        self.profile.as_ref().map(|p| p.frame_generation())
    }

    pub fn reload(&mut self) {
        if let Some(profile) = &mut self.profile {
            let _ = profile.reload();
        }
    }

    /// Sends `url` to the worker as a real navigation. Records `url` as
    /// [`current_url`](Self::current_url) either way, and
    /// [`navigation_error`](Self::navigation_error) if the fetch itself
    /// failed (bad URL, network error, ...) or the worker's response to
    /// the protocol command failed outright (a dead worker).
    pub fn navigate(&mut self, url: &str) {
        self.current_url = url.to_string();
        let Some(profile) = &mut self.profile else {
            return;
        };
        match profile.navigate(url) {
            Ok(Ok(())) => self.navigation_error = None,
            Ok(Err(message)) => self.navigation_error = Some(message),
            Err(io_error) => self.navigation_error = Some(io_error.to_string()),
        }
    }

    /// Uploads a new texture if the worker has published a frame since
    /// the last call, then returns whatever the current texture is (from
    /// this call or a previous one) - `None` only before the very first
    /// frame arrives.
    pub fn poll_texture(&mut self, ctx: &egui::Context) -> Option<&egui::TextureHandle> {
        if let Some(profile) = &self.profile {
            let generation = profile.frame_generation();
            if generation != self.last_generation {
                if let Some(pixels) = profile.latest_frame() {
                    let image = rgba_to_color_image(&pixels, self.width, self.height);
                    let handle = ctx.load_texture("nimble-browser-frame", image, egui::TextureOptions::LINEAR);
                    self.texture = Some(handle);
                    self.last_generation = generation;
                }
            }
        }
        self.texture.as_ref()
    }
}
