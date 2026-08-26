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
  import::import_history(&path)
    .map(|entries| entries.into_iter().map(|e| e.url).collect())
    .map_err(|e| e.to_string())
}

pub fn import_bookmarks(profile_dir: &Path) -> Result<Vec<Bookmark>, String> {
  let path = import::chrome_profile::bookmarks_path(profile_dir);
  import::import_bookmarks(&path).map_err(|e| e.to_string())
}

pub fn default_user_data_dir() -> Option<PathBuf> {
  import::chrome_profile::default_user_data_dir()
}

/// Real DPAPI+AES-256-GCM master key recovery (see `import::master_key`'s
/// own doc) - needed before either cookies or passwords can be decrypted.
pub fn recover_master_key(user_data_dir: &Path) -> Result<Vec<u8>, String> {
  import::recover_master_key(user_data_dir).map_err(|e| e.to_string())
}

pub fn import_cookies(
  profile_dir: &Path,
  master_key: &[u8],
) -> Result<Vec<import::Cookie>, String> {
  let path = import::chrome_profile::cookies_path(profile_dir);
  import::import_cookies(&path, master_key).map_err(|e| e.to_string())
}

pub fn import_passwords(
  profile_dir: &Path,
  master_key: &[u8],
) -> Result<Vec<import::Password>, String> {
  let path = import::chrome_profile::login_data_path(profile_dir);
  import::import_passwords(&path, master_key).map_err(|e| e.to_string())
}

/// The real per-pane storage root a spawned profile's cookies actually
/// live under - mirrors `profile_worker.rs`'s own
/// `temp_dir/nimble-profile-storage/<shmem-name>` construction, which for
/// a stable-identity pane (`browser_view::BrowserView::spawn_with_identity`)
/// is `nimble-profile-<pane-id>`. Kept here (not re-derived ad hoc at each
/// call site) so the one formula has one real home.
pub fn pane_storage_root(pane_id: &str) -> PathBuf {
  std::env::temp_dir()
    .join("nimble-profile-storage")
    .join(format!("nimble-profile-{pane_id}"))
}

/// Writes `cookie` into the selected pane's own real, on-disk
/// `storage::cookies::CookieJar` for its host - the exact file
/// `document.cookie`/`fetch_with_cookies` in a spawned profile already
/// reads from, so an imported cookie is genuinely visible to that pane's
/// pages, not just logged. Builds a real `Set-Cookie` header string and
/// feeds it through `CookieJar::set_from_header` - the same real parser
/// an actual HTTP response would go through - rather than poking at the
/// jar's fields directly.
pub fn write_cookie_into_pane(pane_id: &str, cookie: &import::Cookie) -> std::io::Result<()> {
  let host = cookie.host.trim_start_matches('.');
  let jar_path = pane_storage_root(pane_id).join(host).join("cookies.txt");
  if let Some(parent) = jar_path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let mut jar = storage::cookies::CookieJar::open(&jar_path)?;
  let mut header = format!(
    "{}={}; Domain={}; Path={}",
    cookie.name, cookie.value, cookie.host, cookie.path
  );
  if cookie.is_secure {
    header.push_str("; Secure");
  }
  if cookie.is_http_only {
    header.push_str("; HttpOnly");
  }
  jar.set_from_header(&header, host)
}
