//! Lexer for the spike's minimal grammar — see
//! spec/proposals/ATOMIC_JS_SPIKE.md §5.2. Numbers are plain decimal only
//! (`[0-9]+(\.[0-9]+)?`, no scientific notation/hex/octal/separators);
//! identifiers are ASCII `[A-Za-z_][A-Za-z0-9_]*` — no Unicode identifiers,
//! no `$`. No comments in the grammar — none of the reference programs use
//! them.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Number(f64),
    Identifier(String),
    Function,
    Let,
    Const,
    For,
    If,
    Return,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Semicolon,
    Comma,
    Dot,
    Colon,
    Assign,
    PlusAssign,
    Increment,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Less,
    Greater,
    StarAssign,
    Eof,
}

#[derive(Debug, PartialEq)]
pub struct LexError(pub String);

pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut tokens = Vec::new();

    while i < chars.len() {
        let c = chars[i];

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < chars.len()
                && chars[i] == '.'
                && i + 1 < chars.len()
                && chars[i + 1].is_ascii_digit()
            {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < chars.len() && matches!(chars[i], 'e' | 'E') {
                i += 1;
                if i < chars.len() && matches!(chars[i], '+' | '-') {
                    i += 1;
                }
                let exponent_start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                if exponent_start == i {
                    return Err(LexError("invalid scientific number literal".into()));
                }
            }
            let text: String = chars[start..i].iter().collect();
            let value: f64 = text
                .parse()
                .map_err(|_| LexError(format!("invalid number literal: {text}")))?;
            tokens.push(Token::Number(value));
            continue;
        }

        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            let token = match text.as_str() {
                "function" => Token::Function,
                "let" => Token::Let,
                "const" => Token::Const,
                "for" => Token::For,
                "if" => Token::If,
                "return" => Token::Return,
                _ => Token::Identifier(text),
            };
            tokens.push(token);
            continue;
        }

        match c {
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            '{' => {
                tokens.push(Token::LBrace);
                i += 1;
            }
            '}' => {
                tokens.push(Token::RBrace);
                i += 1;
            }
            ';' => {
                tokens.push(Token::Semicolon);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            '.' => {
                tokens.push(Token::Dot);
                i += 1;
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            '<' => {
                tokens.push(Token::Less);
                i += 1;
            }
            '>' => {
                tokens.push(Token::Greater);
                i += 1;
            }
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::StarAssign);
                    i += 2;
                } else {
                    tokens.push(Token::Star);
                    i += 1;
                }
            }
            '/' => {
                tokens.push(Token::Slash);
                i += 1;
            }
            '%' => {
                tokens.push(Token::Percent);
                i += 1;
            }
            '+' => {
                if i + 1 < chars.len() && chars[i + 1] == '+' {
                    tokens.push(Token::Increment);
                    i += 2;
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    tokens.push(Token::PlusAssign);
                    i += 2;
                } else {
                    tokens.push(Token::Plus);
                    i += 1;
                }
            }
            '-' => {
                tokens.push(Token::Minus);
                i += 1;
            }
            '=' => {
                tokens.push(Token::Assign);
                i += 1;
            }
            other => return Err(LexError(format!("unexpected character: {other:?}"))),
        }
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}
