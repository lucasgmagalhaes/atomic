//! Declaration-block/single-declaration parsing — split out from
//! `parser/mod.rs`. Inherent `impl` methods on [`super::Parser`], same
//! type as `mod.rs`'s own impl block.

use crate::lexer::Token;

use super::types::Declaration;
use super::Parser;

impl<'a> Parser<'a> {
    pub(super) fn parse_declarations(&mut self) -> Vec<Declaration> {
        let mut decls = Vec::new();
        self.skip_whitespace();
        loop {
            match self.tokens.peek() {
                Some(Token::RBrace) | None => {
                    self.tokens.next();
                    break;
                }
                Some(Token::Semicolon) => {
                    self.tokens.next();
                    self.skip_whitespace();
                }
                Some(Token::Ident(_)) => {
                    if let Some(decl) = self.parse_declaration() {
                        decls.push(decl);
                    }
                    self.skip_whitespace();
                }
                _ => {
                    // Unrecoverable token in declaration position - bail
                    // rather than loop forever on malformed input.
                    self.tokens.next();
                }
            }
        }
        decls
    }

    fn parse_declaration(&mut self) -> Option<Declaration> {
        let name = match self.tokens.next()? {
            Token::Ident(name) => name,
            _ => return None,
        };
        self.skip_whitespace();
        if self.tokens.next() != Some(Token::Colon) {
            return None;
        }
        self.skip_whitespace();

        let mut value = Vec::new();
        loop {
            match self.tokens.peek() {
                Some(Token::Semicolon) | Some(Token::RBrace) | None => break,
                Some(Token::Whitespace) => {
                    self.tokens.next();
                }
                _ => value.push(self.tokens.next().unwrap()),
            }
        }
        Some(Declaration { name, value })
    }
}
