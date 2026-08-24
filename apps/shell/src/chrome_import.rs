//! `apps/shell`'s own thin wiring over the real `import` crate - the
//! "opt-in per profile, full import" decision for CLAUDE.md's long-flagged
//! "Import from Chrome" gap. Nothing here runs automatically; every
//! function is only ever called from an explicit Settings-window button
//! click for whichever pane is currently selected - see `main.rs`'s
//! `import_chrome_history_to_selected_pane`/`import_chrome_bookmarks`.
use std::path::{Path, PathBuf};

pub use import::Bookmark;

/// Real Chrome profile discovery (Windows only - see
/// `import::chrome_profile`'s own doc). `None` means there's nowhere to
/// import from on this platform, not a hidden failure.
pub fn default_profile_dir() -> Option<PathBuf> {
    import::chrome_profile::default_profile_dir()
}

/// Real Chrome history import, flattened to just the URLs - `apps/shell`'s
/// own `History` has no title field to carry the rest of what
/// `import::HistoryEntry` provides (see that struct's own doc).
pub fn import_history_urls(profile_dir: &Path) -> Result<Vec<String>, String> {
    let path = import::chrome_profile::history_path(profile_dir);
    import::import_history(&path).map(|entries| entries.into_iter().map(|e| e.url).collect()).map_err(|e| e.to_string())
}

pub fn import_bookmarks(profile_dir: &Path) -> Result<Vec<Bookmark>, String> {
    let path = import::chrome_profile::bookmarks_path(profile_dir);
    import::import_bookmarks(&path).map_err(|e| e.to_string())
}
