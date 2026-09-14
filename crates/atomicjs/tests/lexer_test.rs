use atomicjs::lexer::{tokenize, Token};

mod common;

#[test]
fn tokenizes_integer_and_decimal_numbers() {
    let tokens = tokenize("42 3.14").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Number(42.0), Token::Number(3.14), Token::Eof]
    );
}

#[test]
fn tokenizes_keywords_distinctly_from_identifiers() {
    let tokens = tokenize("function let const for return foo").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Function,
            Token::Let,
            Token::Const,
            Token::For,
            Token::Return,
            Token::Identifier("foo".to_string()),
            Token::Eof,
        ]
    );
}

#[test]
fn tokenizes_identifier_with_underscore_and_digits() {
    let tokens = tokenize("_foo123").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Identifier("_foo123".to_string()), Token::Eof]
    );
}

#[test]
fn tokenizes_punctuation() {
    let tokens = tokenize("(){};,.:").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::LParen,
            Token::RParen,
            Token::LBrace,
            Token::RBrace,
            Token::Semicolon,
            Token::Comma,
            Token::Dot,
            Token::Colon,
            Token::Eof,
        ]
    );
}

#[test]
fn distinguishes_plus_plus_assign_and_increment() {
    let tokens = tokenize("+ += ++").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Plus, Token::PlusAssign, Token::Increment, Token::Eof]
    );
}

#[test]
fn increment_has_no_space_dependent_ambiguity() {
    // `+++` must lex as Increment, Plus — not Plus, Increment — matching the
    // greedy-match order the lexer's `+` branch already uses (checks `++`
    // before `+=` before falling back to plain `+`).
    let tokens = tokenize("+++").unwrap();
    assert_eq!(tokens, vec![Token::Increment, Token::Plus, Token::Eof]);
}

#[test]
fn tokenizes_less_than_and_assign() {
    let tokens = tokenize("< =").unwrap();
    assert_eq!(tokens, vec![Token::Less, Token::Assign, Token::Eof]);
}

#[test]
fn tokenizes_hot_loop_operators_and_scientific_numbers() {
    assert_eq!(
        tokenize("if x > 1e12 { x *= 2 / 3 % 4; }").unwrap(),
        vec![
            Token::If,
            Token::Identifier("x".into()),
            Token::Greater,
            Token::Number(1e12),
            Token::LBrace,
            Token::Identifier("x".into()),
            Token::StarAssign,
            Token::Number(2.0),
            Token::Slash,
            Token::Number(3.0),
            Token::Percent,
            Token::Number(4.0),
            Token::Semicolon,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

#[test]
fn skips_whitespace_including_newlines_and_tabs() {
    let tokens = tokenize("  1\n\t2  ").unwrap();
    assert_eq!(
        tokens,
        vec![Token::Number(1.0), Token::Number(2.0), Token::Eof]
    );
}

#[test]
fn rejects_unexpected_character() {
    let result = tokenize("@");
    assert!(result.is_err());
}

#[test]
fn empty_source_tokenizes_to_just_eof() {
    let tokens = tokenize("").unwrap();
    assert_eq!(tokens, vec![Token::Eof]);
}

#[test]
fn tokenizes_every_reference_program_without_error() {
    for (name, source) in common::all() {
        tokenize(source).unwrap_or_else(|e| panic!("failed to tokenize {name:?}: {e:?}"));
    }
}

#[test]
fn tokenizes_member_access_and_call_shape() {
    let tokens = tokenize("player.damage counter()").unwrap();
    assert_eq!(
        tokens,
        vec![
            Token::Identifier("player".to_string()),
            Token::Dot,
            Token::Identifier("damage".to_string()),
            Token::Identifier("counter".to_string()),
            Token::LParen,
            Token::RParen,
            Token::Eof,
        ]
    );
}
