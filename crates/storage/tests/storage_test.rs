use storage::LocalStorage;

fn temp_path(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("nimble-storage-test-{tag}-{nanos}.txt"))
}

#[test]
fn set_and_get_round_trip() {
    let path = temp_path("basic");
    let mut store = LocalStorage::open(&path).unwrap();
    store.set("theme", "dark").unwrap();
    assert_eq!(store.get("theme"), Some("dark"));
    assert_eq!(store.get("missing"), None);
}

#[test]
fn persists_across_separate_open_calls() {
    let path = temp_path("persist");
    {
        let mut store = LocalStorage::open(&path).unwrap();
        store.set("key", "value").unwrap();
    }
    let reopened = LocalStorage::open(&path).unwrap();
    assert_eq!(reopened.get("key"), Some("value"));
}

#[test]
fn remove_deletes_a_single_key() {
    let path = temp_path("remove");
    let mut store = LocalStorage::open(&path).unwrap();
    store.set("a", "1").unwrap();
    store.set("b", "2").unwrap();
    store.remove("a").unwrap();
    assert_eq!(store.get("a"), None);
    assert_eq!(store.get("b"), Some("2"));
}

#[test]
fn clear_removes_everything_and_persists() {
    let path = temp_path("clear");
    {
        let mut store = LocalStorage::open(&path).unwrap();
        store.set("a", "1").unwrap();
        store.clear().unwrap();
    }
    let reopened = LocalStorage::open(&path).unwrap();
    assert!(reopened.is_empty());
}

#[test]
fn handles_values_containing_newlines_and_backslashes() {
    let path = temp_path("escaping");
    {
        let mut store = LocalStorage::open(&path).unwrap();
        store.set("multiline", "line one\nline two\\with backslash").unwrap();
    }
    let reopened = LocalStorage::open(&path).unwrap();
    assert_eq!(reopened.get("multiline"), Some("line one\nline two\\with backslash"));
}

#[test]
fn opening_a_nonexistent_file_starts_empty_not_erroring() {
    let path = temp_path("fresh");
    let store = LocalStorage::open(&path).unwrap();
    assert!(store.is_empty());
}

#[test]
fn overwriting_an_existing_key_replaces_its_value() {
    let path = temp_path("overwrite");
    let mut store = LocalStorage::open(&path).unwrap();
    store.set("k", "first").unwrap();
    store.set("k", "second").unwrap();
    assert_eq!(store.get("k"), Some("second"));
    assert_eq!(store.len(), 1);
}
