use storage::indexed_db::{CursorDirection, IndexedDb, KeyRange};
use storage::value::Value;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("nimble-cursor-test-{tag}-{nanos}"))
}

fn seeded_store(tag: &str) -> IndexedDb {
    let mut db = IndexedDb::open(temp_dir(tag)).unwrap();
    let store = db.create_object_store("s").unwrap();
    for (k, v) in [("b", "2"), ("a", "1"), ("d", "4"), ("c", "3")] {
        store.put(k, v).unwrap();
    }
    db
}

#[test]
fn cursor_starts_before_the_first_entry() {
    let db = seeded_store("before-first");
    let cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::all(), CursorDirection::Next);
    assert_eq!(
        cursor.key(),
        None,
        "key() before any advance() should be None"
    );
}

#[test]
fn cursor_visits_keys_in_ascending_order_by_default() {
    let db = seeded_store("ascending");
    let mut cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::all(), CursorDirection::Next);

    let mut seen = Vec::new();
    while cursor.advance() {
        seen.push(cursor.key().unwrap().to_string());
    }
    assert_eq!(seen, ["a", "b", "c", "d"]);
}

#[test]
fn cursor_visits_keys_in_descending_order_when_asked() {
    let db = seeded_store("descending");
    let mut cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::all(), CursorDirection::Prev);

    let mut seen = Vec::new();
    while cursor.advance() {
        seen.push(cursor.key().unwrap().to_string());
    }
    assert_eq!(seen, ["d", "c", "b", "a"]);
}

#[test]
fn cursor_value_returns_the_structured_cloned_value_at_the_current_key() {
    let db = seeded_store("value");
    let mut cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::all(), CursorDirection::Next);
    assert!(cursor.advance());
    assert_eq!(cursor.key(), Some("a"));
    assert_eq!(cursor.value(), Some(Value::from("1")));
}

#[test]
fn cursor_returns_false_and_stays_exhausted_past_the_end() {
    let db = seeded_store("exhausted");
    let mut cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::all(), CursorDirection::Next);
    let mut count = 0;
    while cursor.advance() {
        count += 1;
    }
    assert_eq!(count, 4);
    assert!(!cursor.advance());
    assert_eq!(cursor.key(), None);
}

#[test]
fn advance_by_skips_multiple_entries_at_once() {
    let db = seeded_store("advance-by");
    let mut cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::all(), CursorDirection::Next);
    assert!(cursor.advance_by(3));
    assert_eq!(cursor.key(), Some("c"));
}

#[test]
fn advance_by_returns_false_if_it_runs_past_the_end() {
    let db = seeded_store("advance-by-overrun");
    let mut cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::all(), CursorDirection::Next);
    assert!(!cursor.advance_by(10));
}

#[test]
fn key_range_only_visits_the_matching_key() {
    let db = seeded_store("range-only");
    let mut cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::only("b"), CursorDirection::Next);
    assert!(cursor.advance());
    assert_eq!(cursor.key(), Some("b"));
    assert!(!cursor.advance());
}

#[test]
fn key_range_lower_bound_excludes_keys_below_it() {
    let db = seeded_store("range-lower");
    let mut cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::lower_bound("b", false), CursorDirection::Next);
    let mut seen = Vec::new();
    while cursor.advance() {
        seen.push(cursor.key().unwrap().to_string());
    }
    assert_eq!(seen, ["b", "c", "d"]);
}

#[test]
fn key_range_lower_bound_open_excludes_the_bound_itself() {
    let db = seeded_store("range-lower-open");
    let mut cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::lower_bound("b", true), CursorDirection::Next);
    let mut seen = Vec::new();
    while cursor.advance() {
        seen.push(cursor.key().unwrap().to_string());
    }
    assert_eq!(seen, ["c", "d"]);
}

#[test]
fn key_range_bound_restricts_to_a_closed_interval() {
    let db = seeded_store("range-bound");
    let mut cursor = db.store("s").unwrap().open_cursor(
        &KeyRange::bound("b", "c", false, false),
        CursorDirection::Next,
    );
    let mut seen = Vec::new();
    while cursor.advance() {
        seen.push(cursor.key().unwrap().to_string());
    }
    assert_eq!(seen, ["b", "c"]);
}

#[test]
fn empty_store_cursor_never_advances() {
    let mut db = IndexedDb::open(temp_dir("empty")).unwrap();
    db.create_object_store("s").unwrap();
    let mut cursor = db
        .store("s")
        .unwrap()
        .open_cursor(&KeyRange::all(), CursorDirection::Next);
    assert!(!cursor.advance());
}
