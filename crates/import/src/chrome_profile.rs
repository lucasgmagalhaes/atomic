//! Real default Chrome profile directory discovery - Windows only for now
//! (same per-platform-real-or-absent convention as `security::sandbox`/
//! `security::keychain`: `None` on other platforms, not a guessed/fake
//! path that would fail confusingly later).
use std::path::PathBuf;

/// `%LOCALAPPDATA%\Google\Chrome\User Data\Default` - Chrome's real default
/// profile directory on Windows. `None` if `LOCALAPPDATA` isn't set (this
/// function doesn't check the directory actually exists - a caller passing
/// this into [`crate::import_bookmarks`]/[`crate::import_history`] gets a
/// real `Io` error from those if it doesn't, same as any other bad path).
#[cfg(windows)]
pub fn default_profile_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Google").join("Chrome").join("User Data").join("Default"))
}

#[cfg(not(windows))]
pub fn default_profile_dir() -> Option<PathBuf> {
    None
}

pub fn bookmarks_path(profile_dir: &std::path::Path) -> PathBuf {
    profile_dir.join("Bookmarks")
}

pub fn history_path(profile_dir: &std::path::Path) -> PathBuf {
    profile_dir.join("History")
}
