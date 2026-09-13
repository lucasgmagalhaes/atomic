//! `@import`/`@media` parsing — split out from `parser/mod.rs`.

use crate::lexer::Token;

use super::types::{ImportRule, MediaFeature, MediaQuery, Rule};
use super::Parser;

impl<'a> Parser<'a> {
    /// Parses `@import`'s body, already past the `AtKeyword` — either
    /// `"url"` or `url("url")` (only the quoted form; see the lexer's own
    /// "no `url()` token" note), then an optional trailing `@media`-shaped
    /// condition before the terminating `;`. Any tokens between the
    /// (optional) media query and `;` this doesn't recognize are consumed
    /// without failing the whole import, matching this parser's general
    /// "best-effort, don't corrupt the rest of the sheet" stance.
    pub(super) fn parse_import(&mut self) -> Option<ImportRule> {
        self.skip_whitespace();
        let url = match self.tokens.peek().cloned() {
            Some(Token::String(s)) => {
                self.tokens.next();
                s
            }
            Some(Token::Ident(name)) if name.eq_ignore_ascii_case("url") => {
                self.tokens.next();
                self.skip_whitespace();
                if self.tokens.next() != Some(Token::LParen) {
                    return None;
                }
                self.skip_whitespace();
                let url = match self.tokens.next()? {
                    Token::String(s) => s,
                    _ => return None,
                };
                self.skip_whitespace();
                if self.tokens.next() != Some(Token::RParen) {
                    return None;
                }
                url
            }
            _ => return None,
        };

        self.skip_whitespace();
        let media = if matches!(self.tokens.peek(), Some(Token::Semicolon) | None) {
            None
        } else {
            self.parse_media_query()
        };

        while !matches!(self.tokens.peek(), Some(Token::Semicolon) | None) {
            self.tokens.next();
        }
        if self.tokens.peek() == Some(&Token::Semicolon) {
            self.tokens.next();
        }
        Some(ImportRule { url, media })
    }

    /// Parses `@media`'s condition + `{ ... }` block, already past the
    /// `AtKeyword`, pushing every rule inside onto `rules` with the parsed
    /// [`MediaQuery`] attached. Bails (consuming nothing further) if the
    /// block isn't well-formed enough to find an opening `{`.
    pub(super) fn parse_media_block(&mut self, rules: &mut Vec<Rule>) {
        let query = self.parse_media_query();
        self.skip_whitespace();
        if self.tokens.next() != Some(Token::LBrace) {
            return;
        }
        self.skip_whitespace();
        while !matches!(self.tokens.peek(), Some(Token::RBrace) | None) {
            if let Some(rule) = self.parse_rule(query.clone()) {
                rules.push(rule);
            } else {
                self.skip_malformed_rule();
            }
            self.skip_whitespace();
        }
        if self.tokens.peek() == Some(&Token::RBrace) {
            self.tokens.next();
        }
    }

    /// Parses a media query: an optional leading type ident (`screen`,
    /// `print`, ...) optionally followed by `and`, then zero or more
    /// `(feature: value)` conditions joined by `and`. See the module doc
    /// for exactly which features/types are understood — an unsupported
    /// feature is parsed (so the parser stays synchronized) but simply
    /// contributes nothing to the resulting [`MediaQuery`].
    pub(super) fn parse_media_query(&mut self) -> Option<MediaQuery> {
        self.skip_whitespace();
        let mut type_matches = true;

        if let Some(Token::Ident(name)) = self.tokens.peek().cloned() {
            if !name.eq_ignore_ascii_case("and") {
                self.tokens.next();
                let lower = name.to_ascii_lowercase();
                type_matches = lower == "screen" || lower == "all";
                self.skip_whitespace();
                if let Some(Token::Ident(and)) = self.tokens.peek().cloned() {
                    if and.eq_ignore_ascii_case("and") {
                        self.tokens.next();
                    }
                }
            }
        }

        let mut features = Vec::new();
        loop {
            self.skip_whitespace();
            if self.tokens.peek() != Some(&Token::LParen) {
                break;
            }
            self.tokens.next();
            self.skip_whitespace();
            let Some(Token::Ident(feature_name)) = self.tokens.next() else {
                break;
            };
            self.skip_whitespace();

            if self.tokens.peek() == Some(&Token::Colon) {
                self.tokens.next();
                self.skip_whitespace();
                let value = match self.tokens.next() {
                    Some(Token::Dimension(n, unit)) if unit == "px" => Some(n),
                    Some(Token::Number(n)) if n == 0.0 => Some(0.0),
                    _ => None,
                };
                self.skip_whitespace();
                if self.tokens.peek() == Some(&Token::RParen) {
                    self.tokens.next();
                }
                if let Some(value) = value {
                    match feature_name.to_ascii_lowercase().as_str() {
                        "min-width" => features.push(MediaFeature::MinWidth(value)),
                        "max-width" => features.push(MediaFeature::MaxWidth(value)),
                        "width" => features.push(MediaFeature::Width(value)),
                        "min-height" => features.push(MediaFeature::MinHeight(value)),
                        "max-height" => features.push(MediaFeature::MaxHeight(value)),
                        "height" => features.push(MediaFeature::Height(value)),
                        _ => {}
                    }
                }
            } else {
                // Boolean feature form, e.g. `(color)` - or a value shape
                // this parser doesn't understand; skip to the matching `)`.
                while !matches!(self.tokens.peek(), Some(Token::RParen) | None) {
                    self.tokens.next();
                }
                if self.tokens.peek() == Some(&Token::RParen) {
                    self.tokens.next();
                }
            }

            self.skip_whitespace();
            if let Some(Token::Ident(and)) = self.tokens.peek().cloned() {
                if and.eq_ignore_ascii_case("and") {
                    self.tokens.next();
                    continue;
                }
            }
            break;
        }

        Some(MediaQuery {
            type_matches,
            features,
        })
    }
}
