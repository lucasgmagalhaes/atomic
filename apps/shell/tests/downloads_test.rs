//! Real network + real disk I/O, no mocks - same convention as this
//! workspace's other tests that fetch `https://example.com` for real
//! (e.g. `browser_view_test`'s navigation tests).
use shell::downloads::Downloads;

fn temp_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join("nimble-shell-downloads-test").join(name)
}

#[test]
fn download_writes_a_real_file_and_records_a_success() {
    let dir = temp_dir("success");
    let _ = std::fs::remove_dir_all(&dir);
    let mut downloads = Downloads::new(dir.clone());

    let record = downloads.download("https://example.com/");
    let bytes = record.result.as_ref().expect("a real fetch of example.com should succeed");
    assert!(*bytes > 0);
    assert_eq!(record.dest, dir.join("download")); // no path segment in the URL - falls back to "download"
    assert!(record.dest.exists(), "the file should actually be on disk");
    assert_eq!(std::fs::metadata(&record.dest).unwrap().len(), *bytes);

    assert!(!downloads.is_empty());
    assert_eq!(downloads.entries().count(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_download_still_records_an_entry_with_the_error() {
    let dir = temp_dir("failure");
    let _ = std::fs::remove_dir_all(&dir);
    let mut downloads = Downloads::new(dir.clone());

    let record = downloads.download("not-a-real-url");
    assert!(record.result.is_err(), "a bad URL should record Err, not silently succeed");
    assert_eq!(downloads.entries().count(), 1, "a failure should still produce a record");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_reopen_of_the_same_dir_sees_prior_real_download_records() {
    let dir = temp_dir("reopen");
    let _ = std::fs::remove_dir_all(&dir);

    {
        let mut downloads = Downloads::new(dir.clone());
        downloads.download("https://example.com/");
    }

    let reopened = Downloads::new(dir.clone());
    assert_eq!(reopened.entries().count(), 1, "a fresh Downloads over the same dir should load the real manifest left by the previous one");
    let record = reopened.entries().next().unwrap();
    assert_eq!(record.url, "https://example.com/");
    assert!(record.result.as_ref().expect("the persisted record should still show success").clone() > 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn filename_is_derived_from_the_urls_last_path_segment() {
    let dir = temp_dir("filename");
    let _ = std::fs::remove_dir_all(&dir);
    let mut downloads = Downloads::new(dir.clone());

    let record = downloads.download("https://example.com/some/path/file.txt");
    assert_eq!(record.dest, dir.join("file.txt"));

    let _ = std::fs::remove_dir_all(&dir);
}
