//! A minimal, real recursive-descent JSON parser scoped to what Chrome's
//! own `Bookmarks`/`Local State` files actually contain (objects, arrays,
//! strings with standard escapes, numbers, `true`/`false`/`null`) — same
//! "hand-roll the one format this crate needs" convention as `storage`'s
//! own `Value::parse` and `css`'s tokenizer, not a general-purpose JSON
//! library. No `serde_json` dependency anywhere in this workspace yet;
//! this doesn't start one.
use std::collections::BTreeMap;

// `Bool`/`Number` are part of a real, complete JSON value type (this
// parser doesn't special-case which values Chrome's own files happen to
// use) even though `bookmarks.rs` only reads strings/objects/arrays out of
// them today - not literal dead code.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

impl Json {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(map) => map.get(key),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct JsonParseError(pub String);

impl std::fmt::Display for JsonParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON parse error: {}", self.0)
    }
}

impl std::error::Error for JsonParseError {}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn expect(&mut self, b: u8) -> Result<(), JsonParseError> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(JsonParseError(format!("expected '{}' at byte {}", b as char, self.pos)))
        }
    }

    fn parse_value(&mut self) -> Result<Json, JsonParseError> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => Ok(Json::String(self.parse_string()?)),
            Some(b't') => self.parse_literal("true", Json::Bool(true)),
            Some(b'f') => self.parse_literal("false", Json::Bool(false)),
            Some(b'n') => self.parse_literal("null", Json::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.parse_number(),
            other => Err(JsonParseError(format!("unexpected byte {other:?} at {}", self.pos))),
        }
    }

    fn parse_literal(&mut self, literal: &str, value: Json) -> Result<Json, JsonParseError> {
        if self.bytes[self.pos..].starts_with(literal.as_bytes()) {
            self.pos += literal.len();
            Ok(value)
        } else {
            Err(JsonParseError(format!("expected literal {literal} at {}", self.pos)))
        }
    }

    fn parse_number(&mut self) -> Result<Json, JsonParseError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while self.peek().is_some_and(|b| b.is_ascii_digit() || b == b'.' || b == b'e' || b == b'E' || b == b'+' || b == b'-') {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|e| JsonParseError(e.to_string()))?;
        text.parse::<f64>().map(Json::Number).map_err(|e| JsonParseError(e.to_string()))
    }

    fn parse_string(&mut self) -> Result<String, JsonParseError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(JsonParseError("unterminated string".to_string())),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some(b'"') => {
                            out.push('"');
                            self.pos += 1;
                        }
                        Some(b'\\') => {
                            out.push('\\');
                            self.pos += 1;
                        }
                        Some(b'/') => {
                            out.push('/');
                            self.pos += 1;
                        }
                        Some(b'n') => {
                            out.push('\n');
                            self.pos += 1;
                        }
                        Some(b't') => {
                            out.push('\t');
                            self.pos += 1;
                        }
                        Some(b'r') => {
                            out.push('\r');
                            self.pos += 1;
                        }
                        Some(b'b') => {
                            out.push('\u{8}');
                            self.pos += 1;
                        }
                        Some(b'f') => {
                            out.push('\u{c}');
                            self.pos += 1;
                        }
                        Some(b'u') => {
                            self.pos += 1;
                            let hex = std::str::from_utf8(&self.bytes[self.pos..self.pos + 4]).map_err(|e| JsonParseError(e.to_string()))?;
                            let code = u32::from_str_radix(hex, 16).map_err(|e| JsonParseError(e.to_string()))?;
                            out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                            self.pos += 4;
                        }
                        other => return Err(JsonParseError(format!("bad escape {other:?}"))),
                    }
                }
                Some(_) => {
                    // Real UTF-8 decode, not byte-at-a-time - a multi-byte
                    // character must advance by its own real length.
                    let rest = std::str::from_utf8(&self.bytes[self.pos..]).map_err(|e| JsonParseError(e.to_string()))?;
                    let ch = rest.chars().next().expect("checked non-empty above");
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn parse_array(&mut self) -> Result<Json, JsonParseError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                other => return Err(JsonParseError(format!("expected ',' or ']' at {}, got {other:?}", self.pos))),
            }
        }
    }

    fn parse_object(&mut self) -> Result<Json, JsonParseError> {
        self.expect(b'{')?;
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            map.insert(key, value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Object(map));
                }
                other => return Err(JsonParseError(format!("expected ',' or '}}' at {}, got {other:?}", self.pos))),
            }
        }
    }
}

pub fn parse(source: &str) -> Result<Json, JsonParseError> {
    let mut parser = Parser { bytes: source.as_bytes(), pos: 0 };
    let value = parser.parse_value()?;
    parser.skip_ws();
    Ok(value)
}
