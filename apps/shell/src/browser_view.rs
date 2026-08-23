//! Wires the GUI to the real render stack: spawns a `profile::Profile`
//! (a genuine child `profile-worker` process, running its own real
//! per-frame vsync loop and publishing frames over shared memory - see
//! `profile`'s and `profile-worker`'s own docs), polls its latest
//! published frame every repaint, and uploads changed frames as an egui
//! texture. No GPU work happens in this crate - the frame bytes are
//! already fully rendered RGBA8 by the time they reach here; this is
//! purely "display what the profile process already drew."
//!
//! Scoped down from a real browser chrome: one profile, no tabs/
//! workspaces UI wiring yet (`workspace::WorkspaceManager` exists and is
//! tested as a data layer, but nothing here uses it), a fixed frame size
//! decided at spawn time (a real implementation would re-spawn - or
//! resize the shared-memory region - on window resize; this doesn't),
//! and "Reload" is the only chrome control (no address bar - `net`
//! doesn't feed a URL into `profile-worker` yet either, so there's
//! nowhere for an address bar's input to actually go).
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
}

impl BrowserView {
    /// Spawns a fresh profile at `width` x `height`. Failure (missing
    /// binary, shared-memory setup failure, ...) is stored rather than
    /// propagated - the GUI shows it inline instead of failing to start.
    pub fn spawn(width: u32, height: u32) -> Self {
        let shmem_name = format!("nimble-shell-{}", std::process::id());
        let (profile, error) = match worker_binary_path() {
            Ok(path) => match profile::Profile::spawn(&path.to_string_lossy(), &shmem_name, width, height) {
                Ok(p) => (Some(p), None),
                Err(e) => (None, Some(e.to_string())),
            },
            Err(e) => (None, Some(e)),
        };

        BrowserView { profile, texture: None, last_generation: 0, width, height, error }
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn reload(&mut self) {
        if let Some(profile) = &mut self.profile {
            let _ = profile.reload();
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
