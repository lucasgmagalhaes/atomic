use storage::indexed_db::IndexedDb;
use storage::value::Value;

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("nimble-idb-test-{tag}-{nanos}"))
}

#[test]
fn put_and_get_round_trip() {
    let db_path = temp_dir("basic");
    let mut db = IndexedDb::open(&db_path).unwrap();
    let store = db.create_object_store("profiles").unwrap();
    store.put("p1", "alice").unwrap();
    assert_eq!(store.get("p1"), Some(Value::from("alice")));
    assert_eq!(store.get("missing"), None);
}

#[test]
fn put_accepts_a_real_structured_value_tree() {
    let db_path = temp_dir("structured");
    let mut db = IndexedDb::open(&db_path).unwrap();
    let store = db.create_object_store("profiles").unwrap();

    let value = Value::Object(vec![
        ("name".to_string(), Value::from("alice")),
        ("age".to_string(), Value::from(30.0)),
        ("tags".to_string(), Value::Array(vec![Value::from("admin"), Value::from("eng")])),
        ("active".to_string(), Value::Bool(true)),
        ("nickname".to_string(), Value::Null),
    ]);
    store.put("p1", value.clone()).unwrap();

    assert_eq!(store.get("p1"), Some(value));
}

#[test]
fn getting_a_value_returns_an_independent_copy() {
    let db_path = temp_dir("structured-clone");
    let mut db = IndexedDb::open(&db_path).unwrap();
    let store = db.create_object_store("profiles").unwrap();
    store.put("p1", Value::Array(vec![Value::from(1.0)])).unwrap();

    let mut fetched = store.get("p1").unwrap();
    if let Value::Array(items) = &mut fetched {
        items.push(Value::from(2.0));
    }

    // Mutating the fetched copy must not affect what's stored - a real
    // structured clone, not a shared reference.
    assert_eq!(store.get("p1"), Some(Value::Array(vec![Value::from(1.0)])));
    assert_eq!(fetched, Value::Array(vec![Value::from(1.0), Value::from(2.0)]));
}

#[test]
fn multiple_stores_are_independent() {
    let db_path = temp_dir("multi-store");
    let mut db = IndexedDb::open(&db_path).unwrap();
    db.create_object_store("a").unwrap().put("k", "from-a").unwrap();
    db.create_object_store("b").unwrap().put("k", "from-b").unwrap();

    assert_eq!(db.store("a").unwrap().get("k"), Some(Value::from("from-a")));
    assert_eq!(db.store("b").unwrap().get("k"), Some(Value::from("from-b")));
}

#[test]
fn delete_removes_a_key() {
    let db_path = temp_dir("delete");
    let mut db = IndexedDb::open(&db_path).unwrap();
    let store = db.create_object_store("s").unwrap();
    store.put("k", "v").unwrap();
    store.delete("k").unwrap();
    assert_eq!(store.get("k"), None);
}

#[test]
fn clear_empties_a_store() {
    let db_path = temp_dir("clear");
    let mut db = IndexedDb::open(&db_path).unwrap();
    let store = db.create_object_store("s").unwrap();
    store.put("a", "1").unwrap();
    store.put("b", "2").unwrap();
    store.clear().unwrap();
    assert!(store.is_empty());
}

#[test]
fn persists_stores_and_data_across_separate_open_calls() {
    let db_path = temp_dir("persist");
    {
        let mut db = IndexedDb::open(&db_path).unwrap();
        db.create_object_store("profiles").unwrap().put("p1", "alice").unwrap();
        db.create_object_store("settings").unwrap().put("theme", "dark").unwrap();
    }

    let reopened = IndexedDb::open(&db_path).unwrap();
    let mut names: Vec<&str> = reopened.object_store_names().collect();
    names.sort();
    assert_eq!(names, ["profiles", "settings"]);
    assert_eq!(reopened.store("profiles").unwrap().get("p1"), Some(Value::from("alice")));
    assert_eq!(reopened.store("settings").unwrap().get("theme"), Some(Value::from("dark")));
}

#[test]
fn delete_object_store_removes_it_and_its_file() {
    let db_path = temp_dir("delete-store");
    let mut db = IndexedDb::open(&db_path).unwrap();
    db.create_object_store("s").unwrap().put("k", "v").unwrap();
    db.delete_object_store("s").unwrap();

    assert!(db.store("s").is_none());
    let reopened = IndexedDb::open(&db_path).unwrap();
    assert!(reopened.store("s").is_none());
}

#[test]
fn index_finds_keys_by_a_secondary_value() {
    let db_path = temp_dir("index");
    let mut db = IndexedDb::open(&db_path).unwrap();
    db.create_object_store("users").unwrap();
    db.create_index("users", "by_team").unwrap();

    let store = db.store_mut("users").unwrap();
    store.put_indexed("by_team", "u1", "alice", "eng").unwrap();
    store.put_indexed("by_team", "u2", "bob", "eng").unwrap();
    store.put_indexed("by_team", "u3", "carol", "design").unwrap();

    let mut eng: Vec<String> = store
        .get_by_index("by_team", "eng")
        .into_iter()
        .map(|(_, v)| v.as_str().unwrap().to_string())
        .collect();
    eng.sort();
    assert_eq!(eng, ["alice", "bob"]);

    let design = store.get_by_index("by_team", "design");
    assert_eq!(design, vec![("u3", Value::from("carol"))]);

    assert!(store.get_by_index("by_team", "sales").is_empty());
}

#[test]
fn index_persists_across_separate_open_calls() {
    let db_path = temp_dir("index-persist");
    {
        let mut db = IndexedDb::open(&db_path).unwrap();
        db.create_object_store("users").unwrap();
        db.create_index("users", "by_team").unwrap();
        db.store_mut("users").unwrap().put_indexed("by_team", "u1", "alice", "eng").unwrap();
    }

    let reopened = IndexedDb::open(&db_path).unwrap();
    let eng = reopened.store("users").unwrap().get_by_index("by_team", "eng");
    assert_eq!(eng, vec![("u1", Value::from("alice"))]);
}
