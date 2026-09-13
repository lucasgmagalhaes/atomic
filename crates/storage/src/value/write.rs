//! JSON-like text encoding for `Value` — split out from `value.rs`.

use base64::Engine;

use super::Value;

pub(super) fn write_value(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            // JSON has no NaN/Infinity - JSON.stringify(NaN) === "null" is
            // the real spec's own fallback, matched here for round-trip
            // safety even though nothing in this crate produces one yet.
            if n.is_finite() {
                out.push_str(&n.to_string());
            } else {
                out.push_str("null");
            }
        }
        Value::String(s) => write_json_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Value::Object(pairs) => {
            out.push('{');
            for (i, (key, val)) in pairs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json_string(key, out);
                out.push(':');
                write_value(val, out);
            }
            out.push('}');
        }
        // `b"<base64>"` - a distinct token from a plain string (`"..."`),
        // so a real `Value::Bytes` round-trips as itself, not as
        // `Value::String` holding base64 text.
        Value::Bytes(bytes) => {
            out.push('b');
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            write_json_string(&encoded, out);
        }
    }
}

fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
