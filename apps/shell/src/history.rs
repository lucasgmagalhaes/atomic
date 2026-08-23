//! Per-pane browsing history — the "history" half of the mockup's
//! "Downloads & history UI per profile" spec gap. Real persistence now:
//! [`History::open`] loads/appends a newline-delimited file
//! (`<unix-seconds>\t<url>` per line, same convention as `storage`'s own
//! hand-rolled formats in this workspace) under a caller-chosen path -
//! `main.rs` keys it by the pane's own stable id (see
//! `browser_view::BrowserView::spawn_with_identity`'s doc on why that id
//! is now stable across shell relaunches), so a pane's history survives a
//! restart the same way its cookies/localStorage now do. [`History::new`]
//! stays in-memory-only for tests/callers that don't want disk I/O.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct HistoryEntry {
    pub url: String,
    pub at: SystemTime,
}

#[derive(Default)]
pub struct History {
    entries: Vec<HistoryEntry>,
    /// `Some` once opened via [`open`](Self::open) - `record` appends a
    /// line to this file for every real navigation, `None` keeps this
    /// in-memory only (matches the old, pre-persistence behavior).
    path: Option<PathBuf>,
}

fn parse_line(line: &str) -> Option<HistoryEntry> {
    let (secs, url) = line.split_once('\t')?;
    let secs: u64 = secs.parse().ok()?;
    Some(HistoryEntry { url: url.to_string(), at: UNIX_EPOCH + std::time::Duration::from_secs(secs) })
}

fn format_line(entry: &HistoryEntry) -> String {
    let secs = entry.at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    format!("{secs}\t{}", entry.url)
}

impl History {
    pub fn new() -> Self {
        History::default()
    }

    /// Loads whatever real entries already exist at `path` (a missing file
    /// is treated as "no history yet", not an error - the first `record`
    /// creates it) and appends every future [`record`](Self::record) to it.
    /// Malformed lines (corrupted/truncated) are skipped rather than
    /// failing the whole load.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let entries = std::fs::read_to_string(&path).map(|text| text.lines().filter_map(parse_line).collect()).unwrap_or_default();
        History { entries, path: Some(path) }
    }

    /// Records `url` as just-visited. Called from every real navigation
    /// this GUI actually performs (address bar, "Duplicate profile") -
    /// not a placeholder list, a real log of what `profile::Profile::navigate`
    /// was actually asked to load. Appends to the on-disk file too when
    /// [`open`](Self::open)ed with one - a disk-write failure (permissions,
    /// full disk) degrades to "keep the in-memory entry, don't persist it"
    /// rather than losing the record or panicking.
    pub fn record(&mut self, url: impl Into<String>) {
        let entry = HistoryEntry { url: url.into(), at: SystemTime::now() };
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{}", format_line(&entry));
            }
        }
        self.entries.push(entry);
    }

    /// Most recent visit first.
    pub fn entries(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter().rev()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}
