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
use std::str::CharIndices;

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
    self
      .as_object()?
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

fn write_value(value: &Value, out: &mut String) {
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

struct Parser<'a> {
  source: &'a str,
  chars: std::iter::Peekable<CharIndices<'a>>,
}

impl<'a> Parser<'a> {
  fn new(source: &'a str) -> Self {
    Parser {
      source,
      chars: source.char_indices().peekable(),
    }
  }

  fn peek_char(&mut self) -> Option<char> {
    self.chars.peek().map(|&(_, c)| c)
  }

  fn bump(&mut self) -> Option<char> {
    self.chars.next().map(|(_, c)| c)
  }

  fn skip_ws(&mut self) {
    while matches!(self.peek_char(), Some(c) if c.is_whitespace()) {
      self.bump();
    }
  }

  fn expect(&mut self, expected: char) -> Result<(), ParseError> {
    match self.bump() {
      Some(c) if c == expected => Ok(()),
      Some(c) => Err(ParseError::UnexpectedChar(c)),
      None => Err(ParseError::UnexpectedEnd),
    }
  }

  fn expect_literal(&mut self, literal: &str) -> Result<(), ParseError> {
    for expected in literal.chars() {
      self.expect(expected)?;
    }
    Ok(())
  }

  fn parse_value(&mut self) -> Result<Value, ParseError> {
    self.skip_ws();
    match self.peek_char().ok_or(ParseError::UnexpectedEnd)? {
      'n' => {
        self.expect_literal("null")?;
        Ok(Value::Null)
      }
      't' => {
        self.expect_literal("true")?;
        Ok(Value::Bool(true))
      }
      'f' => {
        self.expect_literal("false")?;
        Ok(Value::Bool(false))
      }
      '"' => self.parse_string().map(Value::String),
      '[' => self.parse_array(),
      '{' => self.parse_object(),
      c if c == '-' || c.is_ascii_digit() => self.parse_number(),
      c => Err(ParseError::UnexpectedChar(c)),
    }
  }

  fn parse_string(&mut self) -> Result<String, ParseError> {
    self.expect('"')?;
    let mut out = String::new();
    loop {
      let c = self.bump().ok_or(ParseError::UnexpectedEnd)?;
      match c {
        '"' => return Ok(out),
        '\\' => {
          let escaped = self.bump().ok_or(ParseError::UnexpectedEnd)?;
          match escaped {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
              let mut code = 0u32;
              for _ in 0..4 {
                let digit = self.bump().ok_or(ParseError::UnexpectedEnd)?;
                code = code * 16 + digit.to_digit(16).ok_or(ParseError::InvalidEscape)?;
              }
              out.push(char::from_u32(code).ok_or(ParseError::InvalidEscape)?);
            }
            _ => return Err(ParseError::InvalidEscape),
          }
        }
        c => out.push(c),
      }
    }
  }

  fn parse_number(&mut self) -> Result<Value, ParseError> {
    let start = self
      .chars
      .peek()
      .map(|&(i, _)| i)
      .unwrap_or(self.source.len());
    if self.peek_char() == Some('-') {
      self.bump();
    }
    while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
      self.bump();
    }
    if self.peek_char() == Some('.') {
      self.bump();
      while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
        self.bump();
      }
    }
    if matches!(self.peek_char(), Some('e') | Some('E')) {
      self.bump();
      if matches!(self.peek_char(), Some('+') | Some('-')) {
        self.bump();
      }
      while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
        self.bump();
      }
    }
    let end = self
      .chars
      .peek()
      .map(|&(i, _)| i)
      .unwrap_or(self.source.len());
    self.source[start..end]
      .parse::<f64>()
      .map(Value::Number)
      .map_err(|_| ParseError::InvalidNumber)
  }

  fn parse_array(&mut self) -> Result<Value, ParseError> {
    self.expect('[')?;
    let mut items = Vec::new();
    self.skip_ws();
    if self.peek_char() == Some(']') {
      self.bump();
      return Ok(Value::Array(items));
    }
    loop {
      items.push(self.parse_value()?);
      self.skip_ws();
      match self.bump() {
        Some(',') => continue,
        Some(']') => return Ok(Value::Array(items)),
        Some(c) => return Err(ParseError::UnexpectedChar(c)),
        None => return Err(ParseError::UnexpectedEnd),
      }
    }
  }

  fn parse_object(&mut self) -> Result<Value, ParseError> {
    self.expect('{')?;
    let mut pairs = Vec::new();
    self.skip_ws();
    if self.peek_char() == Some('}') {
      self.bump();
      return Ok(Value::Object(pairs));
    }
    loop {
      self.skip_ws();
      let key = self.parse_string()?;
      self.skip_ws();
      self.expect(':')?;
      let value = self.parse_value()?;
      pairs.push((key, value));
      self.skip_ws();
      match self.bump() {
        Some(',') => continue,
        Some('}') => return Ok(Value::Object(pairs)),
        Some(c) => return Err(ParseError::UnexpectedChar(c)),
        None => return Err(ParseError::UnexpectedEnd),
      }
    }
  }
}
