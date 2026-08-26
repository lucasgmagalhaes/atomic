use shell::history::History;

fn temp_path(name: &str) -> std::path::PathBuf {
  std::env::temp_dir()
    .join("nimble-shell-history-test")
    .join(name)
}

#[test]
fn open_on_a_missing_file_starts_empty() {
  let path = temp_path("missing.txt");
  let _ = std::fs::remove_file(&path);
  let history = History::open(&path);
  assert!(history.is_empty());
}

#[test]
fn record_persists_to_disk_and_a_reopen_sees_it() {
  let path = temp_path("persists.txt");
  let _ = std::fs::remove_file(&path);

  {
    let mut history = History::open(&path);
    history.record("https://example.com/");
    history.record("https://example.org/");
  }

  let reopened = History::open(&path);
  let urls: Vec<&str> = reopened.entries().map(|e| e.url.as_str()).collect();
  assert_eq!(
    urls,
    vec!["https://example.org/", "https://example.com/"],
    "a reopened history should see real entries from the previous open, most recent first"
  );

  let _ = std::fs::remove_file(&path);
}

#[test]
fn starts_empty() {
  let history = History::new();
  assert!(history.is_empty());
  assert_eq!(history.entries().count(), 0);
}

#[test]
fn record_appends_and_entries_returns_most_recent_first() {
  let mut history = History::new();
  history.record("https://example.com/");
  history.record("https://example.org/");

  assert!(!history.is_empty());
  let urls: Vec<&str> = history.entries().map(|e| e.url.as_str()).collect();
  assert_eq!(urls, vec!["https://example.org/", "https://example.com/"]);
}
