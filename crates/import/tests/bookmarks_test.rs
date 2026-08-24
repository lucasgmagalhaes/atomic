//! Real file I/O against a genuine Chrome-shaped `Bookmarks` JSON file -
//! no mocks, matches this workspace's own testing convention.
use import::import_bookmarks;

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join("nimble-import-test").join(name)
}

const REAL_CHROME_BOOKMARKS_SHAPE: &str = r#"{
  "checksum": "abc123",
  "roots": {
    "bookmark_bar": {
      "type": "folder",
      "name": "Bookmarks bar",
      "children": [
        {
          "type": "url",
          "name": "Example",
          "url": "https://example.com/"
        },
        {
          "type": "folder",
          "name": "Work",
          "children": [
            {
              "type": "url",
              "name": "Nested \"quoted\" name",
              "url": "https://example.org/nested"
            }
          ]
        }
      ]
    },
    "other": {
      "type": "folder",
      "name": "Other bookmarks",
      "children": []
    },
    "synced": {
      "type": "folder",
      "name": "Mobile bookmarks",
      "children": []
    }
  },
  "version": 1
}"#;

#[test]
fn imports_every_real_bookmark_with_its_folder_path() {
    let path = temp_path("Bookmarks");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, REAL_CHROME_BOOKMARKS_SHAPE).unwrap();

    let bookmarks = import_bookmarks(&path).expect("a real Chrome-shaped file should import");
    assert_eq!(bookmarks.len(), 2);

    let top = bookmarks.iter().find(|b| b.url == "https://example.com/").expect("top-level bookmark should be found");
    assert_eq!(top.name, "Example");
    assert_eq!(top.folder, "Bookmarks bar");

    let nested = bookmarks.iter().find(|b| b.url == "https://example.org/nested").expect("nested bookmark should be found");
    assert_eq!(nested.name, "Nested \"quoted\" name", "a real quote-escaped name should decode correctly");
    assert_eq!(nested.folder, "Bookmarks bar/Work");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_missing_file_reports_a_real_io_error() {
    let path = temp_path("does-not-exist");
    let _ = std::fs::remove_file(&path);
    let result = import_bookmarks(&path);
    assert!(result.is_err());
}

#[test]
fn a_file_without_the_real_roots_shape_is_rejected_not_silently_empty() {
    let path = temp_path("not-bookmarks.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, r#"{"hello": "world"}"#).unwrap();

    let result = import_bookmarks(&path);
    assert!(result.is_err(), "a JSON file that isn't shaped like Chrome's Bookmarks should error, not return an empty list");

    let _ = std::fs::remove_file(&path);
}
