//! Per-pane download list — the "downloads" half of the mockup's
//! "Downloads & history UI per profile" spec gap. Real transfers via
//! `net::download` (already-real HTTP/TLS GET-to-disk - see that crate),
//! not a placeholder list. Same in-memory, per-`Pane`, not-persisted
//! scope as `history` - see that module's doc for why.
//!
//! Files land under a dedicated OS temp subdirectory
//! (`%TEMP%/nimble-downloads/<pane-id>/`), not the user's real Downloads
//! folder - this is agent-authored code the user runs themselves, so
//! there's no "Claude downloaded a file" consent question here, but a
//! browser feature landing files somewhere so specific to this project's
//! own scratch space (rather than a well-known user folder) still needs a
//! real per-profile UI for "open containing folder" before it'd be worth
//! pointing at anything else - not built here.
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct DownloadRecord {
    pub url: String,
    pub dest: PathBuf,
    /// `Ok(bytes)` on success, `Err(message)` if the fetch/write failed -
    /// a failed download still gets a record (so the user sees it failed,
    /// not silence), same "no silent failure" convention `pane.fill`/
    /// `.click` already follow elsewhere in this workspace.
    pub result: Result<u64, String>,
    pub at: Instant,
}

pub struct Downloads {
    dir: PathBuf,
    entries: Vec<DownloadRecord>,
}

/// The last non-empty path segment of `url`, percent-decoded is NOT
/// attempted (this engine has no URL-decoding utility beyond the `url`
/// crate's own, and a raw segment is a fine enough filename for this
/// scope) - `"download"` if the URL has no path segment at all (e.g. a
/// bare `https://example.com/`).
fn filename_from_url(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.path_segments().and_then(|mut s| s.next_back()).filter(|s| !s.is_empty()).map(str::to_string))
        .unwrap_or_else(|| "download".to_string())
}

impl Downloads {
    /// `dir` is created lazily on the first real download, not here - a
    /// pane that never downloads anything never touches the filesystem.
    pub fn new(dir: PathBuf) -> Self {
        Downloads { dir, entries: Vec::new() }
    }

    /// Fetches `url` for real via `net::download`, writing to
    /// `self.dir/<filename derived from the URL>` (a name collision
    /// within one pane's own download dir overwrites the earlier file -
    /// no de-duplication/renaming in this pass). Always appends a record,
    /// success or failure, and returns it.
    pub fn download(&mut self, url: &str) -> &DownloadRecord {
        let _ = std::fs::create_dir_all(&self.dir);
        let dest = self.dir.join(filename_from_url(url));
        // `net::download` empties `response.body` after writing it to
        // disk (see that function's own doc - no reason to hold a second
        // in-memory copy) - the real byte count comes from the file it
        // just wrote, not the response.
        let result = net::download(url, &dest)
            .map_err(|e| e.to_string())
            .and_then(|_| std::fs::metadata(&dest).map(|m| m.len()).map_err(|e| e.to_string()));
        self.entries.push(DownloadRecord { url: url.to_string(), dest, result, at: Instant::now() });
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
