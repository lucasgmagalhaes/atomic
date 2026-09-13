//! Selector + declaration parser built on [`crate::lexer`]. Combinators:
//! descendant (whitespace), child (`>`), next-sibling (`+`), and
//! subsequent-sibling (`~`) between compound selectors. Attribute
//! selectors: `[attr]`/`[attr=value]` (no `~=`/`^=`/`$=`/`*=`/`|=`
//! variants, no case-insensitivity flag). Pseudo-classes: `:first-child`,
//! `:last-child`, `:nth-child(...)` (`odd`/`even`/an integer/`An+B`, see
//! [`NthFormula`]) are real, matched against sibling position; `:hover`/
//! `:focus` parse but always evaluate `false` — there's no real
//! hover/focus state tracked anywhere in this engine yet (no input-event
//! plumbing reaches here), so rather than fake them firing this documents
//! the gap honestly, same convention as `sandbox::confine`'s
//! `Unsupported` on non-Windows. No pseudo-elements (`::before`), no
//! `:not()`/other functional pseudo-classes beyond `:nth-child`.
//! Declaration values are kept as raw [`Token`]s, not parsed into typed
//! CSS values (lengths, colors, ...) — that's cascade/layout's job, once
//! they exist to consume it.
//!
//! Also handles two `@`-rules for real: `@media` ([`MediaQuery`], attached
//! to every [`Rule`] parsed inside its block — [`crate::matching_declarations`]
//! filters on it) and `@import` (collected into [`Stylesheet::imports`] as
//! [`ImportRule`]s for a caller with network access to resolve/fetch/merge,
//! same division of labor this crate already has with `<link>` extraction
//! living in `profile-worker`, not here). Any other `@`-rule (`@font-face`,
//! `@keyframes`, `@charset`, ...) is recognized and skipped without
//! corrupting the rest of the parse — its prelude and body (a `{...}`
//! block if it has one, tracking nested-brace depth) are just consumed and
//! discarded, not interpreted.
//!
//! `@media`'s own scope: features are `px`-only `min-width`/`max-width`/
//! `width` (matches this workspace's own px-only length model), ANDed
//! together within one query (`screen and (min-width: 600px)`); no
//! comma-separated query lists (`OR` semantics), no nesting inside another
//! `@media`, no features beyond the three width ones (`prefers-color-scheme`,
//! `orientation`, `hover`, ... are parsed as a value-less/skipped feature
//! and ignored). A media type other than `screen`/`all`/unspecified (e.g.
//! `print`) makes the whole query never match — this engine only ever has
//! one rendering context, and it isn't print.
//!
//! Split into `types.rs` (every selector/declaration/rule/stylesheet
//! type), `selectors.rs` (selector-list/complex/compound/`:nth-child`
//! parsing), and `declarations.rs` (declaration-block parsing) — this
//! file keeps the module doc, the `Parser` struct, its stylesheet/rule/
//! at-rule-level parsing methods, and the single public entry point,
//! [`parse_stylesheet`].

use crate::lexer::{Lexer, Token};

mod at_rules;
mod declarations;
mod selectors;
mod types;

pub use types::{
    AttributeMatch, AttributeSelector, Combinator, ComplexSelector, CompoundSelector, Declaration,
    ImportRule, MediaFeature, MediaQuery, NthFormula, PseudoClass, Rule, SelectorList,
    SimpleSelector, Stylesheet,
};

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
        let mut imports = Vec::new();
        self.skip_whitespace();
        while let Some(tok) = self.tokens.peek().cloned() {
            match tok {
                Token::AtKeyword(name) if name.eq_ignore_ascii_case("import") => {
                    self.tokens.next();
                    if let Some(import) = self.parse_import() {
                        imports.push(import);
                    }
                }
                Token::AtKeyword(name) if name.eq_ignore_ascii_case("media") => {
                    self.tokens.next();
                    self.parse_media_block(&mut rules);
                }
                Token::AtKeyword(_) => {
                    self.tokens.next();
                    self.skip_at_rule_body();
                }
                _ => {
                    if let Some(rule) = self.parse_rule(None) {
                        rules.push(rule);
                    } else {
                        self.skip_malformed_rule();
                    }
                }
            }
            self.skip_whitespace();
        }
        Stylesheet { rules, imports }
    }

    /// Real CSS error recovery for a rule `parse_rule` failed partway
    /// through (an unsupported selector construct - an unrecognized
    /// pseudo-class like `:link`/`:visited`, or any other selector syntax
    /// outside this parser's real feature set - real-world CSS is full of
    /// these). Without this, the outer loop would retry `parse_rule` on
    /// the exact same stuck token forever: a failed selector parse can
    /// leave the cursor mid-selector (having already consumed some real
    /// tokens before hitting the unsupported one) or unmoved entirely, and
    /// neither `parse_rule` nor its own failure path guarantees forward
    /// progress on its own — this is what does. Matches the CSS Syntax
    /// Module's own "consume the remnants of a bad qualified rule"
    /// recovery: skip forward to the next `{` (discarding whatever's
    /// between here and there - the rest of the malformed selector), then
    /// skip that block's real balanced-brace contents too (same tracking
    /// `skip_at_rule_body` already does for at-rules). No `{` before EOF
    /// just drains to the end.
    fn skip_malformed_rule(&mut self) {
        loop {
            match self.tokens.next() {
                Some(Token::LBrace) => {
                    let mut depth = 1;
                    while depth > 0 {
                        match self.tokens.next() {
                            Some(Token::LBrace) => depth += 1,
                            Some(Token::RBrace) => depth -= 1,
                            Some(_) => {}
                            None => break,
                        }
                    }
                    break;
                }
                Some(_) => {}
                None => break,
            }
        }
    }

    fn parse_rule(&mut self, media: Option<MediaQuery>) -> Option<Rule> {
        let selectors = self.parse_selector_list()?;
        if self.tokens.next() != Some(Token::LBrace) {
            return None;
        }
        let declarations = self.parse_declarations();
        Some(Rule {
            selectors,
            declarations,
            media,
        })
    }

    /// Consumes an unrecognized `@`-rule's prelude and body (already past
    /// the `AtKeyword` itself): up to a terminating `;` (no body, e.g.
    /// `@charset "utf-8";`) or a balanced `{...}` block (tracking nested
    /// brace depth, since a block-having at-rule's body can itself contain
    /// braces, e.g. `@keyframes`'s percentage steps).
    fn skip_at_rule_body(&mut self) {
        loop {
            match self.tokens.peek() {
                Some(Token::Semicolon) => {
                    self.tokens.next();
                    break;
                }
                Some(Token::LBrace) => {
                    self.tokens.next();
                    let mut depth = 1;
                    while depth > 0 {
                        match self.tokens.next() {
                            Some(Token::LBrace) => depth += 1,
                            Some(Token::RBrace) => depth -= 1,
                            Some(_) => {}
                            None => break,
                        }
                    }
                    break;
                }
                Some(_) => {
                    self.tokens.next();
                }
                None => break,
            }
        }
    }
}

pub fn parse_stylesheet(input: &str) -> Stylesheet {
    Parser::new(input).parse_stylesheet()
}
