//! Real import of Chrome's `Bookmarks` file (JSON, under a profile
//! directory - e.g. `%LOCALAPPDATA%\Google\Chrome\User Data\Default\
//! Bookmarks` on Windows) - the "opt-in per profile, full import" the user
//! chose for CLAUDE.md's long-flagged "Import from Chrome" gap. Real
//! recursive walk of Chrome's own `roots.bookmark_bar`/`other`/`synced`
//! trees (`type: "folder"` nodes nest `children`, `type: "url"` nodes carry
//! `url`) - not a flattened guess at the format.
use crate::json::{parse, Json, JsonParseError};
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub name: String,
    pub url: String,
    /// Slash-joined folder path from whichever root it was found under
    /// (e.g. `"Bookmarks bar/Work"`) - real structure preserved, not
    /// discarded during the walk.
    pub folder: String,
}

#[derive(Debug)]
pub enum ImportError {
    Io(std::io::Error),
    Parse(JsonParseError),
    /// The file parsed as JSON but doesn't have the `roots` shape a real
    /// Chrome `Bookmarks` file always has - a real signal this isn't
    /// actually a Chrome bookmarks file, not silently returning nothing.
    UnexpectedShape(String),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Io(e) => write!(f, "failed to read bookmarks file: {e}"),
            ImportError::Parse(e) => write!(f, "failed to parse bookmarks file: {e}"),
            ImportError::UnexpectedShape(s) => write!(
                f,
                "bookmarks file doesn't look like a real Chrome export: {s}"
            ),
        }
    }
}

impl std::error::Error for ImportError {}

fn walk(node: &Json, folder: &str, out: &mut Vec<Bookmark>) {
    let Some(node_type) = node.get("type").and_then(Json::as_str) else {
        return;
    };
    match node_type {
        "url" => {
            let name = node
                .get("name")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            let url = node
                .get("url")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            if !url.is_empty() {
                out.push(Bookmark {
                    name,
                    url,
                    folder: folder.to_string(),
                });
            }
        }
        "folder" => {
            let name = node.get("name").and_then(Json::as_str).unwrap_or("");
            let child_folder = if folder.is_empty() {
                name.to_string()
            } else {
                format!("{folder}/{name}")
            };
            if let Some(children) = node.get("children").and_then(Json::as_array) {
                for child in children {
                    walk(child, &child_folder, out);
                }
            }
        }
        _ => {}
    }
}

/// Real, tested against a genuine Chrome `Bookmarks` file's shape (see
/// `tests/bookmarks_test.rs`) - every entry Chrome itself would show
/// under Bookmarks Manager, in whatever folder it's actually nested under.
pub fn import_bookmarks(path: &Path) -> Result<Vec<Bookmark>, ImportError> {
    let text = std::fs::read_to_string(path).map_err(ImportError::Io)?;
    let root = parse(&text).map_err(ImportError::Parse)?;
    let roots = root.get("roots").ok_or_else(|| {
        ImportError::UnexpectedShape("missing top-level \"roots\" object".to_string())
    })?;
    let Json::Object(roots_map) = roots else {
        return Err(ImportError::UnexpectedShape(
            "\"roots\" is not an object".to_string(),
        ));
    };

    // Root keys ("bookmark_bar", "other", "synced") are Chrome's internal
    // ids, not what a user sees - `walk` builds the real folder path from
    // each node's own "name" field instead, starting empty here so the
    // root's own name ("Bookmarks bar", ...) isn't duplicated.
    let mut out = Vec::new();
    for root_node in roots_map.values() {
        walk(root_node, "", &mut out);
    }
    Ok(out)
}
