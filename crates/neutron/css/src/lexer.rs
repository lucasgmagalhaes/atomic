//! CSS tokenizer. A practical subset of CSS Syntax Module Level 3, not a
//! full implementation — no `url()` token (a quoted URL still works, it's
//! just a plain `String` token inside `url(...)`'s parens — see the
//! parser's `@import` handling), no unicode-range, no string-escape error
//! recovery, no bad-string/bad-url tokens. Enough for the selector +
//! declaration parser this crate needs, plus `@`-rules (`AtKeyword`, for
//! `@import`/`@media`); extend as real stylesheets need it.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    /// `#foo` (id selector or hex color) — the text after `#`.
    Hash(String),
    /// `.foo`'s `.` — class selectors are `Delim('.')` followed by `Ident`.
    Delim(char),
    String(String),
    Number(f64),
    /// A number immediately followed by an ident, e.g. `10px`, `1.5em`.
    Dimension(f64, String),
    Percentage(f64),
    /// `@import`, `@media`, `@foo` — the name after `@`. Only recognized
    /// when `@` is immediately followed by an identifier start; a bare
    /// `@` with nothing ident-like after it falls through to `Delim('@')`.
    AtKeyword(String),
    Colon,
    Semicolon,
    Comma,
    LBrace,
    RBrace,
    LParen,
    RParen,
    Whitespace,
}

pub struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Lexer {
            chars: input.chars().peekable(),
        }
    }

    fn skip_comment(&mut self) -> bool {
        // Caller already confirmed the leading `/*`.
        self.chars.next();
        self.chars.next();
        while let Some(c) = self.chars.next() {
            if c == '*' && self.chars.peek() == Some(&'/') {
                self.chars.next();
                return true;
            }
        }
        true
    }

    fn read_while<F: Fn(char) -> bool>(&mut self, pred: F) -> String {
        let mut s = String::new();
        while let Some(&c) = self.chars.peek() {
            if pred(c) {
                s.push(c);
                self.chars.next();
            } else {
                break;
            }
        }
        s
    }

    fn read_string(&mut self, quote: char) -> String {
        self.chars.next(); // opening quote
        let mut s = String::new();
        while let Some(&c) = self.chars.peek() {
            self.chars.next();
            if c == quote {
                break;
            }
            if c == '\\' {
                if let Some(&next) = self.chars.peek() {
                    s.push(next);
                    self.chars.next();
                }
                continue;
            }
            s.push(c);
        }
        s
    }

    fn read_number(&mut self) -> f64 {
        let s = self.read_while(|c| c.is_ascii_digit() || c == '.');
        s.parse().unwrap_or(0.0)
    }

    fn next_token(&mut self) -> Option<Token> {
        let &c = self.chars.peek()?;

        if c.is_whitespace() {
            self.read_while(|c| c.is_whitespace());
            return Some(Token::Whitespace);
        }
        if c == '/' {
            let mut clone = self.chars.clone();
            clone.next();
            if clone.peek() == Some(&'*') {
                self.skip_comment();
                return self.next_token();
            }
        }
        if c == '"' || c == '\'' {
            return Some(Token::String(self.read_string(c)));
        }
        if c.is_ascii_digit() || (c == '.' && self.peek_is_digit_after_dot()) {
            let n = self.read_number();
            return Some(self.number_suffix(n));
        }
        if c == '#' {
            self.chars.next();
            let name = self.read_while(is_ident_char);
            return Some(Token::Hash(name));
        }
        if c == '@' {
            self.chars.next();
            let next = self.chars.peek().copied();
            let next_starts_ident = matches!(next, Some(n) if self.starts_ident(n));
            if next_starts_ident {
                let name = self.read_while(is_ident_char);
                return Some(Token::AtKeyword(name));
            }
            return Some(Token::Delim('@'));
        }
        if self.starts_ident(c) {
            let name = self.read_while(is_ident_char);
            return Some(Token::Ident(name));
        }

        self.chars.next();
        Some(match c {
            ':' => Token::Colon,
            ';' => Token::Semicolon,
            ',' => Token::Comma,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '(' => Token::LParen,
            ')' => Token::RParen,
            other => Token::Delim(other),
        })
    }

    fn peek_is_digit_after_dot(&self) -> bool {
        let mut clone = self.chars.clone();
        clone.next();
        matches!(clone.peek(), Some(c) if c.is_ascii_digit())
    }

    /// A `-` only starts an identifier (e.g. `-webkit-foo`, `--custom-prop`)
    /// when followed by a letter, `_`, or another `-` — not a digit, which
    /// belongs to a (possibly-unary-minus'd, but we don't fold that here)
    /// number instead. Every other ident-start character has no such
    /// lookahead requirement.
    fn starts_ident(&self, c: char) -> bool {
        if c != '-' {
            return is_ident_start(c);
        }
        let mut clone = self.chars.clone();
        clone.next();
        matches!(clone.peek(), Some(&n) if n == '-' || n == '_' || n.is_alphabetic())
    }

    fn number_suffix(&mut self, n: f64) -> Token {
        if self.chars.peek() == Some(&'%') {
            self.chars.next();
            return Token::Percentage(n);
        }
        if matches!(self.chars.peek(), Some(&c) if is_ident_start(c)) {
            let unit = self.read_while(is_ident_char);
            return Token::Dimension(n, unit);
        }
        Token::Number(n)
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_' || c == '-'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

impl Iterator for Lexer<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Token> {
        self.next_token()
    }
}

/// Tokenizes `input`, dropping whitespace tokens — callers that need
/// whitespace-sensitivity (e.g. distinguishing `.a .b` from `.a.b`) should
/// use [`Lexer`] directly instead.
pub fn tokenize(input: &str) -> Vec<Token> {
    Lexer::new(input)
        .filter(|t| *t != Token::Whitespace)
        .collect()
}
