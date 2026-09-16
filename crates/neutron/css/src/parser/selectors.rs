//! Selector-list/complex-selector/compound-selector/`:nth-child` parsing —
//! split out from `parser/mod.rs`. Inherent `impl` methods on
//! [`super::Parser`], same type as `mod.rs`'s own impl block.

use crate::lexer::Token;

use super::types::{
    AttributeMatch, AttributeSelector, Combinator, ComplexSelector, CompoundSelector, NthFormula,
    PseudoClass, SelectorList, SimpleSelector,
};
use super::Parser;

impl<'a> Parser<'a> {
    pub(super) fn parse_selector_list(&mut self) -> Option<SelectorList> {
        let mut list = vec![self.parse_complex_selector()?];
        self.skip_whitespace();
        while self.tokens.peek() == Some(&Token::Comma) {
            self.tokens.next();
            self.skip_whitespace();
            list.push(self.parse_complex_selector()?);
            self.skip_whitespace();
        }
        Some(SelectorList(list))
    }

    fn parse_complex_selector(&mut self) -> Option<ComplexSelector> {
        let mut compounds = vec![self.parse_compound_selector()?];
        let mut combinators = Vec::new();
        loop {
            let saw_whitespace = self.skip_whitespace_report();
            match self.tokens.peek() {
                Some(Token::Comma) | Some(Token::LBrace) | None => break,
                Some(Token::Delim('>')) => {
                    self.tokens.next();
                    self.skip_whitespace();
                    combinators.push(Combinator::Child);
                    compounds.push(self.parse_compound_selector()?);
                }
                Some(Token::Delim('+')) => {
                    self.tokens.next();
                    self.skip_whitespace();
                    combinators.push(Combinator::NextSibling);
                    compounds.push(self.parse_compound_selector()?);
                }
                Some(Token::Delim('~')) => {
                    self.tokens.next();
                    self.skip_whitespace();
                    combinators.push(Combinator::SubsequentSibling);
                    compounds.push(self.parse_compound_selector()?);
                }
                _ if saw_whitespace => {
                    combinators.push(Combinator::Descendant);
                    compounds.push(self.parse_compound_selector()?);
                }
                _ => break,
            }
        }
        Some(ComplexSelector(compounds, combinators))
    }

    fn parse_compound_selector(&mut self) -> Option<CompoundSelector> {
        let mut simples = Vec::new();
        loop {
            match self.tokens.peek() {
                Some(Token::Ident(name)) => {
                    simples.push(SimpleSelector::Type(name.clone()));
                    self.tokens.next();
                }
                Some(Token::Hash(id)) => {
                    simples.push(SimpleSelector::Id(id.clone()));
                    self.tokens.next();
                }
                Some(Token::Delim('.')) => {
                    self.tokens.next();
                    if let Some(Token::Ident(name)) = self.tokens.next() {
                        simples.push(SimpleSelector::Class(name));
                    } else {
                        return None;
                    }
                }
                Some(Token::Delim('*')) => {
                    simples.push(SimpleSelector::Universal);
                    self.tokens.next();
                }
                Some(Token::Delim('[')) => {
                    self.tokens.next();
                    self.skip_whitespace();
                    let Some(Token::Ident(name)) = self.tokens.next() else {
                        return None;
                    };
                    self.skip_whitespace();
                    let match_ = if self.tokens.peek() == Some(&Token::Delim('=')) {
                        self.tokens.next();
                        self.skip_whitespace();
                        let value = match self.tokens.next()? {
                            Token::String(s) => s,
                            Token::Ident(s) => s,
                            _ => return None,
                        };
                        AttributeMatch::Equals(value)
                    } else {
                        AttributeMatch::Has
                    };
                    self.skip_whitespace();
                    if self.tokens.next() != Some(Token::Delim(']')) {
                        return None;
                    }
                    simples.push(SimpleSelector::Attribute(AttributeSelector {
                        name,
                        match_,
                    }));
                }
                Some(Token::Colon) => {
                    self.tokens.next();
                    let Some(Token::Ident(name)) = self.tokens.next() else {
                        return None;
                    };
                    let pseudo = match name.to_ascii_lowercase().as_str() {
                        "first-child" => PseudoClass::FirstChild,
                        "last-child" => PseudoClass::LastChild,
                        "hover" => PseudoClass::Hover,
                        "focus" => PseudoClass::Focus,
                        "nth-child" => {
                            if self.tokens.next() != Some(Token::LParen) {
                                return None;
                            }
                            let formula = self.parse_nth_formula()?;
                            self.skip_whitespace();
                            if self.tokens.next() != Some(Token::RParen) {
                                return None;
                            }
                            PseudoClass::NthChild(formula)
                        }
                        _ => return None,
                    };
                    simples.push(SimpleSelector::PseudoClass(pseudo));
                }
                _ => break,
            }
        }
        if simples.is_empty() {
            None
        } else {
            Some(CompoundSelector(simples))
        }
    }

    /// Parses the `An+B` expression inside `:nth-child(...)`, already past
    /// the opening `(`. See [`NthFormula`]'s doc for the exact grammar
    /// this understands — `odd`, `even`, a bare integer, or `<int>n`
    /// optionally followed by `+<int>`/`-<int>`. Whitespace around any of
    /// these is tolerated (the CSS grammar allows it).
    fn parse_nth_formula(&mut self) -> Option<NthFormula> {
        self.skip_whitespace();
        match self.tokens.peek().cloned() {
            Some(Token::Ident(name)) if name.eq_ignore_ascii_case("odd") => {
                self.tokens.next();
                Some(NthFormula { a: 2, b: 1 })
            }
            Some(Token::Ident(name)) if name.eq_ignore_ascii_case("even") => {
                self.tokens.next();
                Some(NthFormula { a: 2, b: 0 })
            }
            Some(Token::Number(n)) => {
                self.tokens.next();
                Some(NthFormula { a: 0, b: n as i64 })
            }
            Some(Token::Dimension(n, unit)) if unit.eq_ignore_ascii_case("n") => {
                self.tokens.next();
                let b = self.parse_optional_b_term()?;
                Some(NthFormula { a: n as i64, b })
            }
            Some(Token::Ident(name)) if name.eq_ignore_ascii_case("n") => {
                self.tokens.next();
                let b = self.parse_optional_b_term()?;
                Some(NthFormula { a: 1, b })
            }
            Some(Token::Ident(name)) if name.eq_ignore_ascii_case("-n") => {
                self.tokens.next();
                let b = self.parse_optional_b_term()?;
                Some(NthFormula { a: -1, b })
            }
            _ => None,
        }
    }

    /// The optional `+<int>`/`-<int>` tail of an `An+B` expression, past
    /// the `n`/`An` part. `0` when there's no such tail (a bare `n`).
    fn parse_optional_b_term(&mut self) -> Option<i64> {
        self.skip_whitespace();
        match self.tokens.peek().cloned() {
            Some(Token::Delim('+')) => {
                self.tokens.next();
                self.skip_whitespace();
                match self.tokens.next()? {
                    Token::Number(n) => Some(n as i64),
                    _ => None,
                }
            }
            Some(Token::Delim('-')) => {
                self.tokens.next();
                self.skip_whitespace();
                match self.tokens.next()? {
                    Token::Number(n) => Some(-(n as i64)),
                    _ => None,
                }
            }
            _ => Some(0),
        }
    }
}
