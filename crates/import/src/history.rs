//! Real import of Chrome's `History` file (a genuine SQLite database, under
//! the same profile directory as `Bookmarks` - e.g. `...\User Data\
//! Default\History`) - real `rusqlite` (bundled real SQLite, not a
//! hand-rolled reader; this workspace hand-rolls its own small protocols
//! elsewhere but SQLite's on-disk format is not one of those - same
//! "use a real vetted crate for a complex external format" call as `hyper`/
//! `wgpu`/`html5ever` already make).
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub visit_count: i64,
    /// Real wall-clock time, converted from Chrome's own epoch (WebKit
    /// time: microseconds since 1601-01-01 UTC) to Unix time - not left in
    /// Chrome's native units, which nothing else in this workspace uses.
    pub last_visit_time: std::time::SystemTime,
}

#[derive(Debug)]
pub enum ImportError {
    Io(std::io::Error),
    Sqlite(rusqlite::Error),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Io(e) => write!(f, "failed to prepare history file for import: {e}"),
            ImportError::Sqlite(e) => write!(f, "failed to read history database: {e}"),
        }
    }
}

impl std::error::Error for ImportError {}

/// Microseconds between the WebKit/Chrome epoch (1601-01-01 00:00:00 UTC)
/// and the Unix epoch (1970-01-01 00:00:00 UTC) - the standard constant
/// for this conversion (86400 * (369 years of days incl. leap days)).
const WEBKIT_EPOCH_OFFSET_MICROS: i64 = 11_644_473_600_000_000;

fn webkit_time_to_system_time(webkit_micros: i64) -> std::time::SystemTime {
    let unix_micros = webkit_micros - WEBKIT_EPOCH_OFFSET_MICROS;
    if unix_micros >= 0 {
        std::time::UNIX_EPOCH + std::time::Duration::from_micros(unix_micros as u64)
    } else {
        std::time::UNIX_EPOCH - std::time::Duration::from_micros((-unix_micros) as u64)
    }
}

/// Real, tested against a genuine SQLite database with Chrome's own
/// `urls` table schema (see `tests/history_test.rs`, which creates one for
/// real via `rusqlite` rather than mocking a reader).
///
/// Copies `path` to a temp file before opening it - a real running Chrome
/// holds its own `History` file open (SQLite's file locking would
/// otherwise make a live import fail or block), so this reads a real
/// point-in-time snapshot instead, the same workaround real browser-import
/// tools use.
pub fn import_history(path: &Path) -> Result<Vec<HistoryEntry>, ImportError> {
    let temp_path = std::env::temp_dir().join(format!(
        "nimble-import-history-{}.sqlite",
        std::process::id()
    ));
    std::fs::copy(path, &temp_path).map_err(ImportError::Io)?;

    let result = (|| {
        let conn = rusqlite::Connection::open(&temp_path).map_err(ImportError::Sqlite)?;
        let mut stmt = conn
            .prepare("SELECT url, title, visit_count, last_visit_time FROM urls")
            .map_err(ImportError::Sqlite)?;
        let rows = stmt
            .query_map([], |row| {
                let url: String = row.get(0)?;
                let title: String = row.get(1)?;
                let visit_count: i64 = row.get(2)?;
                let last_visit_time: i64 = row.get(3)?;
                Ok(HistoryEntry {
                    url,
                    title,
                    visit_count,
                    last_visit_time: webkit_time_to_system_time(last_visit_time),
                })
            })
            .map_err(ImportError::Sqlite)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(ImportError::Sqlite)
    })();

    let _ = std::fs::remove_file(&temp_path);
    result
}
