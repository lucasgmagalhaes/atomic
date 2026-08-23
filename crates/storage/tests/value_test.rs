use storage::value::{ParseError, Value};

#[test]
fn round_trips_every_primitive_kind() {
    for value in [Value::Null, Value::Bool(true), Value::Bool(false), Value::from(3.5), Value::from(-42.0), Value::from("hello")] {
        let wire = value.to_wire();
        assert_eq!(Value::parse(&wire).unwrap(), value, "failed to round-trip {wire:?}");
    }
}

#[test]
fn round_trips_nested_arrays_and_objects() {
    let value = Value::Object(vec![
        ("name".to_string(), Value::from("alice")),
        ("scores".to_string(), Value::Array(vec![Value::from(1.0), Value::from(2.5), Value::Null])),
        ("meta".to_string(), Value::Object(vec![("active".to_string(), Value::Bool(true))])),
    ]);
    let wire = value.to_wire();
    assert_eq!(Value::parse(&wire).unwrap(), value);
}

#[test]
fn string_escapes_round_trip() {
    let value = Value::from("line one\nline two\ttabbed \"quoted\" back\\slash");
    let wire = value.to_wire();
    assert_eq!(Value::parse(&wire).unwrap(), value);
}

#[test]
fn parses_a_plain_json_literal_written_by_hand() {
    // Not produced by `to_wire` - a literal a human/JS caller might write,
    // to prove the parser isn't just the exact inverse of the encoder.
    let parsed = Value::parse(r#"{"a": [1, 2.5, -3, true, false, null], "b": "hi"}"#).unwrap();
    assert_eq!(
        parsed,
        Value::Object(vec![
            ("a".to_string(), Value::Array(vec![Value::from(1.0), Value::from(2.5), Value::from(-3.0), Value::Bool(true), Value::Bool(false), Value::Null])),
            ("b".to_string(), Value::from("hi")),
        ])
    );
}

#[test]
fn parses_unicode_escapes() {
    let parsed = Value::parse(r#""café""#).unwrap();
    assert_eq!(parsed, Value::from("café"));
}

#[test]
fn rejects_malformed_input() {
    assert!(matches!(Value::parse(""), Err(ParseError::UnexpectedEnd)));
    assert!(matches!(Value::parse("{"), Err(ParseError::UnexpectedEnd)));
    assert!(matches!(Value::parse("[1, 2"), Err(ParseError::UnexpectedEnd)));
    assert!(matches!(Value::parse("nul"), Err(_)));
    assert!(matches!(Value::parse("123 456"), Err(ParseError::TrailingData)));
    assert!(matches!(Value::parse("{\"a\" 1}"), Err(ParseError::UnexpectedChar('1'))));
}

#[test]
fn accessors_narrow_to_the_right_variant() {
    assert_eq!(Value::from("hi").as_str(), Some("hi"));
    assert_eq!(Value::from(1.0).as_str(), None);
    assert_eq!(Value::from(3.0).as_f64(), Some(3.0));
    assert_eq!(Value::Bool(true).as_bool(), Some(true));

    let obj = Value::Object(vec![("k".to_string(), Value::from("v"))]);
    assert_eq!(obj.get("k"), Some(&Value::from("v")));
    assert_eq!(obj.get("missing"), None);
    assert_eq!(Value::from("not an object").get("k"), None);
}

#[test]
fn clone_deep_produces_an_independent_copy() {
    let original = Value::Array(vec![Value::from(1.0)]);
    let mut cloned = original.clone_deep();
    if let Value::Array(items) = &mut cloned {
        items.push(Value::from(2.0));
    }
    assert_eq!(original, Value::Array(vec![Value::from(1.0)]));
    assert_eq!(cloned, Value::Array(vec![Value::from(1.0), Value::from(2.0)]));
}
