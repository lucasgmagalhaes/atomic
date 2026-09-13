//! Recursive-descent parser for `Value`'s JSON-like wire format — split
//! out from `value.rs`.

use std::str::CharIndices;

use super::{ParseError, Value};

pub(super) struct Parser<'a> {
    source: &'a str,
    chars: std::iter::Peekable<CharIndices<'a>>,
}

impl<'a> Parser<'a> {
    pub(super) fn new(source: &'a str) -> Self {
        Parser {
            source,
            chars: source.char_indices().peekable(),
        }
    }

    pub(super) fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().map(|&(_, c)| c)
    }

    fn bump(&mut self) -> Option<char> {
        self.chars.next().map(|(_, c)| c)
    }

    pub(super) fn skip_ws(&mut self) {
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

    pub(super) fn parse_value(&mut self) -> Result<Value, ParseError> {
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
                                code = code * 16
                                    + digit.to_digit(16).ok_or(ParseError::InvalidEscape)?;
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
