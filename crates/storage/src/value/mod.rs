//! A structured-clone-shaped value tree for IndexedDB — real recursive
//! values (null/bool/number/string/array/object), not the opaque strings
//! `ObjectStore` stored before this. Real (de)serialization too: a
//! hand-rolled recursive-descent parser/encoder over a JSON-like text
//! form (matching this workspace's "no serde" convention — same reasoning
//! as `css`'s hand-rolled lexer/parser), used only as this value tree's
//! on-disk wire format.
//!
//! Deviations from the real Structured Clone Algorithm: no `Date`, `Map`,
//! `Set`, `ArrayBuffer`/typed arrays, `RegExp`, or circular-reference
//! support (a real structured clone can contain cycles; this tree can't
//! represent one at all, so there's nothing to break) — those need either
//! more variants or a cycle-aware clone algorithm this crate doesn't need
//! yet. `clone_deep` is real regardless: every `Value` here is already a
//! fully-owned tree with no shared/aliased state, so an ordinary `.clone()`
//! *is* a structured clone — no separate deep-copy pass required, unlike
//! JS's own object graphs.
//!
//! Split into `write.rs` (JSON-like text encoding) and `parse.rs`
//! (the recursive-descent parser).

mod parse;
mod write;

use parse::Parser;
use write::write_value;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    /// Insertion-ordered, like a real JS object's own enumeration order —
    /// a `HashMap` would silently scramble it.
    Object(Vec<(String, Value)>),
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Looks up a key in an `Object` value. `None` for a non-object or a
    /// missing key alike.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// A real deep/independent copy — see the module doc for why this is
    /// just `.clone()` for this type.
    pub fn clone_deep(&self) -> Value {
        self.clone()
    }

    /// Encodes this value as JSON-like text.
    pub fn to_wire(&self) -> String {
        let mut out = String::new();
        write_value(self, &mut out);
        out
    }

    pub fn parse(source: &str) -> Result<Value, ParseError> {
        let mut parser = Parser::new(source);
        parser.skip_ws();
        let value = parser.parse_value()?;
        parser.skip_ws();
        if parser.peek_char().is_some() {
            return Err(ParseError::TrailingData);
        }
        Ok(value)
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    UnexpectedEnd,
    UnexpectedChar(char),
    InvalidNumber,
    InvalidEscape,
    TrailingData,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnexpectedEnd => write!(f, "unexpected end of input"),
            ParseError::UnexpectedChar(c) => write!(f, "unexpected character '{c}'"),
            ParseError::InvalidNumber => write!(f, "invalid number literal"),
            ParseError::InvalidEscape => write!(f, "invalid string escape"),
            ParseError::TrailingData => write!(f, "trailing data after value"),
        }
    }
}

impl std::error::Error for ParseError {}
