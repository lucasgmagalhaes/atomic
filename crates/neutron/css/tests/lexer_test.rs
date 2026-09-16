use css::{tokenize, Token};

#[test]
fn tokenizes_simple_rule() {
    let tokens = tokenize("div { color: red; }");
    assert_eq!(
        tokens,
        vec![
            Token::Ident("div".into()),
            Token::LBrace,
            Token::Ident("color".into()),
            Token::Colon,
            Token::Ident("red".into()),
            Token::Semicolon,
            Token::RBrace,
        ]
    );
}

#[test]
fn tokenizes_id_and_class_selectors() {
    let tokens = tokenize("#main .item {}");
    assert_eq!(
        tokens,
        vec![
            Token::Hash("main".into()),
            Token::Delim('.'),
            Token::Ident("item".into()),
            Token::LBrace,
            Token::RBrace,
        ]
    );
}

#[test]
fn tokenizes_dimensions_and_percentages() {
    let tokens = tokenize("width: 10.5px; height: 50%;");
    assert_eq!(
        tokens,
        vec![
            Token::Ident("width".into()),
            Token::Colon,
            Token::Dimension(10.5, "px".into()),
            Token::Semicolon,
            Token::Ident("height".into()),
            Token::Colon,
            Token::Percentage(50.0),
            Token::Semicolon,
        ]
    );
}

#[test]
fn tokenizes_negative_dimension_as_delim_then_dimension() {
    // No unary-minus handling at the tokenizer level (matches the CSS
    // Syntax spec: `-` only starts a number when immediately followed by
    // a digit or `.digit`, which this lexer doesn't special-case yet).
    let tokens = tokenize("-10px");
    assert_eq!(
        tokens,
        vec![Token::Delim('-'), Token::Dimension(10.0, "px".into())]
    );
}

#[test]
fn tokenizes_quoted_strings_with_escapes() {
    let tokens = tokenize(r#"content: "a\"b";"#);
    assert_eq!(
        tokens,
        vec![
            Token::Ident("content".into()),
            Token::Colon,
            Token::String("a\"b".into()),
            Token::Semicolon,
        ]
    );
}

#[test]
fn skips_comments() {
    let tokens = tokenize("div /* comment */ { }");
    assert_eq!(
        tokens,
        vec![Token::Ident("div".into()), Token::LBrace, Token::RBrace]
    );
}

#[test]
fn tokenizes_plain_number() {
    let tokens = tokenize("opacity: 0.5;");
    assert_eq!(
        tokens,
        vec![
            Token::Ident("opacity".into()),
            Token::Colon,
            Token::Number(0.5),
            Token::Semicolon,
        ]
    );
}
