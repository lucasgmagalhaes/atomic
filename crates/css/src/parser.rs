//! Selector + declaration parser built on [`crate::lexer`]. Scoped subset:
//! only the descendant combinator (whitespace) between compound selectors —
//! no `>`, `+`, `~`, no pseudo-classes/elements, no attribute selectors.
//! Declaration values are kept as raw [`Token`]s, not parsed into typed
//! CSS values (lengths, colors, ...) — that's cascade/layout's job, once
//! they exist to consume it.
use crate::lexer::{Lexer, Token};

#[derive(Debug, Clone, PartialEq)]
pub enum SimpleSelector {
    Universal,
    Type(String),
    Id(String),
    Class(String),
}

/// A run of simple selectors with no combinator between them, e.g.
/// `div.foo#bar` → `[Type("div"), Class("foo"), Id("bar")]`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompoundSelector(pub Vec<SimpleSelector>);

/// Compound selectors joined by descendant combinators, e.g. `div .item`.
#[derive(Debug, Clone, PartialEq)]
pub struct ComplexSelector(pub Vec<CompoundSelector>);

impl ComplexSelector {
    /// `(id count, class count, type count)` — CSS specificity without the
    /// separate "inline style"/"important" tiers, which don't apply to a
    /// selector in isolation.
    pub fn specificity(&self) -> (u32, u32, u32) {
        let mut spec = (0, 0, 0);
        for compound in &self.0 {
            for simple in &compound.0 {
                match simple {
                    SimpleSelector::Id(_) => spec.0 += 1,
                    SimpleSelector::Class(_) => spec.1 += 1,
                    SimpleSelector::Type(_) => spec.2 += 1,
                    SimpleSelector::Universal => {}
                }
            }
        }
        spec
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectorList(pub Vec<ComplexSelector>);

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub name: String,
    pub value: Vec<Token>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selectors: SelectorList,
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

struct Parser<'a> {
    tokens: std::iter::Peekable<Lexer<'a>>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser {
            tokens: Lexer::new(input).peekable(),
        }
    }

    fn skip_whitespace(&mut self) {
        while self.tokens.peek() == Some(&Token::Whitespace) {
            self.tokens.next();
        }
    }

    /// Like [`Self::skip_whitespace`], but reports whether it skipped
    /// anything — used by the selector parser, where whitespace between
    /// compound selectors is the descendant combinator, not just filler.
    fn skip_whitespace_report(&mut self) -> bool {
        let mut saw_any = false;
        while self.tokens.peek() == Some(&Token::Whitespace) {
            self.tokens.next();
            saw_any = true;
        }
        saw_any
    }

    fn parse_stylesheet(&mut self) -> Stylesheet {
        let mut rules = Vec::new();
        self.skip_whitespace();
        while self.tokens.peek().is_some() {
            if let Some(rule) = self.parse_rule() {
                rules.push(rule);
            }
            self.skip_whitespace();
        }
        Stylesheet { rules }
    }

    fn parse_rule(&mut self) -> Option<Rule> {
        let selectors = self.parse_selector_list()?;
        if self.tokens.next() != Some(Token::LBrace) {
            return None;
        }
        let declarations = self.parse_declarations();
        Some(Rule {
            selectors,
            declarations,
        })
    }

    fn parse_selector_list(&mut self) -> Option<SelectorList> {
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
        loop {
            let saw_whitespace = self.skip_whitespace_report();
            match self.tokens.peek() {
                Some(Token::Comma) | Some(Token::LBrace) | None => break,
                _ if saw_whitespace => {
                    compounds.push(self.parse_compound_selector()?);
                }
                _ => break,
            }
        }
        Some(ComplexSelector(compounds))
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
                _ => break,
            }
        }
        if simples.is_empty() {
            None
        } else {
            Some(CompoundSelector(simples))
        }
    }

    fn parse_declarations(&mut self) -> Vec<Declaration> {
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

pub fn parse_stylesheet(input: &str) -> Stylesheet {
    Parser::new(input).parse_stylesheet()
}
