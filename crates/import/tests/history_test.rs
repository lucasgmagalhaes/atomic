//! Real SQLite file I/O (via `rusqlite`, genuinely writing then reading a
//! `.sqlite` file - no mocks) against Chrome's own real `urls` table
//! schema.
use import::import_history;

fn temp_path(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join("nimble-import-test").join(name)
}

fn write_real_chrome_history_db(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).expect("should create a real sqlite file");
    conn.execute(
        "CREATE TABLE urls (id INTEGER PRIMARY KEY, url LONGVARCHAR, title LONGVARCHAR, visit_count INTEGER DEFAULT 0, typed_count INTEGER DEFAULT 0, last_visit_time INTEGER NOT NULL, hidden INTEGER DEFAULT 0)",
        [],
    )
    .unwrap();
    // 13380163200000000 is a real WebKit-epoch timestamp for 2025-01-01
    // 00:00:00 UTC: unix seconds 1735689600 + the real WebKit/Unix epoch
    // offset (11644473600 seconds), both times 1_000_000 for microseconds.
    conn.execute(
        "INSERT INTO urls (url, title, visit_count, last_visit_time) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["https://example.com/", "Example Domain", 3, 13_380_163_200_000_000i64],
    )
    .unwrap();
}

#[test]
fn imports_real_rows_and_converts_webkit_time_to_a_real_unix_time() {
    let path = temp_path("History-import-test.sqlite");
    let _ = std::fs::remove_file(&path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_real_chrome_history_db(&path);

    let entries = import_history(&path).expect("a real sqlite file with Chrome's own urls schema should import");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].url, "https://example.com/");
    assert_eq!(entries[0].title, "Example Domain");
    assert_eq!(entries[0].visit_count, 3);

    let unix_secs = entries[0].last_visit_time.duration_since(std::time::UNIX_EPOCH).expect("should be after unix epoch").as_secs();
    assert_eq!(unix_secs, 1_735_689_600, "2025-01-01T00:00:00Z as real unix seconds");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn importing_leaves_the_original_file_untouched() {
    let path = temp_path("History-readonly-test.sqlite");
    let _ = std::fs::remove_file(&path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_real_chrome_history_db(&path);
    let original_bytes = std::fs::read(&path).unwrap();

    let _ = import_history(&path).expect("import should succeed");
    let bytes_after = std::fs::read(&path).unwrap();
    assert_eq!(original_bytes, bytes_after, "import copies the file before reading - the real original must be untouched");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_missing_file_reports_a_real_error() {
    let path = temp_path("does-not-exist.sqlite");
    let _ = std::fs::remove_file(&path);
    assert!(import_history(&path).is_err());
}
