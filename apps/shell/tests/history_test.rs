use shell::history::History;

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
