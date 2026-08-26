//! Per-pane download list — the "downloads" half of the mockup's
//! "Downloads & history UI per profile" spec gap. Real transfers via
//! `net::download` (already-real HTTP/TLS GET-to-disk - see that crate),
//! not a placeholder list.
//!
//! Files land under a dedicated OS temp subdirectory
//! (`%TEMP%/nimble-downloads/<pane-id>/`), not the user's real Downloads
//! folder - this is agent-authored code the user runs themselves, so
//! there's no "Claude downloaded a file" consent question here, but a
//! browser feature landing files somewhere so specific to this project's
//! own scratch space (rather than a well-known user folder) still needs a
//! real per-profile UI for "open containing folder" before it'd be worth
//! pointing at anything else - not built here.
//!
//! The record *list* itself is real and persisted too now: `new(dir)`
//! transparently loads a manifest file (`dir/.downloads.manifest`, one
//! tab-separated line per past download) if one already exists there, and
//! `download` appends to it - since `dir` is now keyed by a pane's stable
//! id (see `browser_view::BrowserView::spawn_with_identity`'s doc), the
//! list survives a shell restart the same way `history` now does. A
//! caller that always passes a fresh/unique `dir` (like this crate's own
//! tests) sees no difference from the old in-memory-only behavior.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct DownloadRecord {
  pub url: String,
  pub dest: PathBuf,
  /// `Ok(bytes)` on success, `Err(message)` if the fetch/write failed -
  /// a failed download still gets a record (so the user sees it failed,
  /// not silence), same "no silent failure" convention `pane.fill`/
  /// `.click` already follow elsewhere in this workspace.
  pub result: Result<u64, String>,
  pub at: SystemTime,
}

pub struct Downloads {
  dir: PathBuf,
  entries: Vec<DownloadRecord>,
}

const MANIFEST_FILE: &str = ".downloads.manifest";

/// The last non-empty path segment of `url`, percent-decoded is NOT
/// attempted (this engine has no URL-decoding utility beyond the `url`
/// crate's own, and a raw segment is a fine enough filename for this
/// scope) - `"download"` if the URL has no path segment at all (e.g. a
/// bare `https://example.com/`).
fn filename_from_url(url: &str) -> String {
  url::Url::parse(url)
    .ok()
    .and_then(|u| {
      u.path_segments()
        .and_then(|mut s| s.next_back())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    })
    .unwrap_or_else(|| "download".to_string())
}

fn parse_manifest_line(line: &str) -> Option<DownloadRecord> {
  let mut parts = line.splitn(4, '\t');
  let secs: u64 = parts.next()?.parse().ok()?;
  let url = parts.next()?.to_string();
  let dest = PathBuf::from(parts.next()?);
  let status = parts.next()?;
  let result = match status.strip_prefix("OK:") {
    Some(bytes) => Ok(bytes.parse().ok()?),
    None => Err(status.strip_prefix("ERR:")?.to_string()),
  };
  Some(DownloadRecord {
    url,
    dest,
    result,
    at: UNIX_EPOCH + Duration::from_secs(secs),
  })
}

fn format_manifest_line(record: &DownloadRecord) -> String {
  let secs = record
    .at
    .duration_since(UNIX_EPOCH)
    .unwrap_or_default()
    .as_secs();
  let status = match &record.result {
    Ok(bytes) => format!("OK:{bytes}"),
    Err(message) => format!("ERR:{}", message.replace(['\n', '\t'], " ")),
  };
  format!(
    "{secs}\t{}\t{}\t{status}",
    record.url,
    record.dest.display()
  )
}

impl Downloads {
  /// `dir` is created lazily on the first real download, not here - a
  /// pane that never downloads anything never touches the filesystem.
  /// If `dir/.downloads.manifest` already exists (a real prior run under
  /// the same stable pane id), its entries are loaded immediately - see
  /// this module's own doc.
  pub fn new(dir: PathBuf) -> Self {
    let entries = std::fs::read_to_string(dir.join(MANIFEST_FILE))
      .map(|text| text.lines().filter_map(parse_manifest_line).collect())
      .unwrap_or_default();
    Downloads { dir, entries }
  }

  /// Fetches `url` for real via `net::download`, writing to
  /// `self.dir/<filename derived from the URL>` (a name collision
  /// within one pane's own download dir overwrites the earlier file -
  /// no de-duplication/renaming in this pass). Always appends a record,
  /// success or failure, and returns it. Also appends the same record to
  /// the on-disk manifest - a write failure there (permissions, full
  /// disk) degrades to "keep the in-memory entry, don't persist it"
  /// rather than losing the download itself or panicking.
  pub fn download(&mut self, url: &str) -> &DownloadRecord {
    let _ = std::fs::create_dir_all(&self.dir);
    let dest = self.dir.join(filename_from_url(url));
    // `net::download` empties `response.body` after writing it to
    // disk (see that function's own doc - no reason to hold a second
    // in-memory copy) - the real byte count comes from the file it
    // just wrote, not the response.
    let result = net::download(url, &dest)
      .map_err(|e| e.to_string())
      .and_then(|_| {
        std::fs::metadata(&dest)
          .map(|m| m.len())
          .map_err(|e| e.to_string())
      });
    let record = DownloadRecord {
      url: url.to_string(),
      dest,
      result,
      at: SystemTime::now(),
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
      .create(true)
      .append(true)
      .open(self.dir.join(MANIFEST_FILE))
    {
      let _ = writeln!(file, "{}", format_manifest_line(&record));
    }
    self.entries.push(record);
    self.entries.last().expect("just pushed")
  }

  /// Most recent download first.
  pub fn entries(&self) -> impl Iterator<Item = &DownloadRecord> {
    self.entries.iter().rev()
  }

  pub fn is_empty(&self) -> bool {
    self.entries.is_empty()
  }

  pub fn dir(&self) -> &Path {
    &self.dir
  }
}
