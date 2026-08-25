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
use crate::lexer::{Lexer, Token};

/// `[attr]` vs `[attr=value]` — see the module doc for the variants this
/// doesn't cover.
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeMatch {
    Has,
    Equals(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeSelector {
    pub name: String,
    pub match_: AttributeMatch,
}

/// A parsed `An+B` expression from `:nth-child(...)` — `odd`/`even` are
/// sugar for `(2,1)`/`(2,0)`, a bare integer `B` is `(0,B)`. [`Self::matches`]
/// takes a 1-based sibling position (matching the CSS spec's own
/// 1-indexing), true iff some non-negative integer `k` solves
/// `position == a*k + b`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NthFormula {
    pub a: i64,
    pub b: i64,
}

impl NthFormula {
    pub fn matches(&self, position: i64) -> bool {
        if self.a == 0 {
            return position == self.b;
        }
        let diff = position - self.b;
        diff % self.a == 0 && diff / self.a >= 0
    }
}

/// See the module doc for exactly what's real (`:first-child`/
/// `:last-child`/`:nth-child`) vs. an honest always-`false` stand-in
/// (`:hover`/`:focus`).
#[derive(Debug, Clone, PartialEq)]
pub enum PseudoClass {
    FirstChild,
    LastChild,
    NthChild(NthFormula),
    Hover,
    Focus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SimpleSelector {
    Universal,
    Type(String),
    Id(String),
    Class(String),
    Attribute(AttributeSelector),
    PseudoClass(PseudoClass),
}

/// A run of simple selectors with no combinator between them, e.g.
/// `div.foo#bar` → `[Type("div"), Class("foo"), Id("bar")]`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CompoundSelector(pub Vec<SimpleSelector>);

/// The relationship between two adjacent compound selectors in a
/// [`ComplexSelector`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Combinator {
    /// Whitespace: right-hand compound matches any descendant.
    Descendant,
    /// `>`: right-hand compound matches only the immediate child.
    Child,
    /// `+`: right-hand compound matches only the immediately following
    /// sibling under the same parent.
    NextSibling,
    /// `~`: right-hand compound matches any later sibling under the same
    /// parent.
    SubsequentSibling,
}

/// Compound selectors joined by combinators, e.g. `div > .item + span`.
/// `.1[i]` is the combinator immediately to the left of `.0[i + 1]` — so
/// `.1.len() == .0.len() - 1` for any non-empty selector.
#[derive(Debug, Clone, PartialEq)]
pub struct ComplexSelector(pub Vec<CompoundSelector>, pub Vec<Combinator>);

impl ComplexSelector {
    /// `(id count, class count, type count)` — CSS specificity without the
    /// separate "inline style"/"important" tiers, which don't apply to a
    /// selector in isolation. Attribute selectors and pseudo-classes both
    /// count at the "class" tier, matching the real spec (only
    /// pseudo-*elements*, which this crate doesn't have, would count at
    /// the type tier).
    pub fn specificity(&self) -> (u32, u32, u32) {
        let mut spec = (0, 0, 0);
        for compound in &self.0 {
            for simple in &compound.0 {
                match simple {
                    SimpleSelector::Id(_) => spec.0 += 1,
                    SimpleSelector::Class(_)
                    | SimpleSelector::Attribute(_)
                    | SimpleSelector::PseudoClass(_) => spec.1 += 1,
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

/// One `px`-only media feature condition — see the module doc's `@media`
/// scope note.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MediaFeature {
    MinWidth(f64),
    MaxWidth(f64),
    Width(f64),
}

/// A parsed `@media` condition: an optional media-type gate plus a set of
/// features ANDed together. See the module doc for exactly what's
/// supported.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaQuery {
    /// `false` for a named media type this engine never matches (e.g.
    /// `print`) — short-circuits [`matches`](Self::matches) regardless of
    /// `features`. `true` for `screen`, `all`, or no type specified.
    pub type_matches: bool,
    pub features: Vec<MediaFeature>,
}

impl MediaQuery {
    pub fn matches(&self, viewport_width: f64) -> bool {
        self.type_matches
            && self.features.iter().all(|f| match f {
                MediaFeature::MinWidth(w) => viewport_width >= *w,
                MediaFeature::MaxWidth(w) => viewport_width <= *w,
                MediaFeature::Width(w) => (viewport_width - w).abs() < 0.001,
            })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selectors: SelectorList,
    pub declarations: Vec<Declaration>,
    /// `None` for a rule outside any `@media` block (always eligible,
    /// pending normal selector matching) — `Some` for one parsed inside
    /// one, checked by [`crate::matching_declarations`] before selector
    /// matching even runs.
    pub media: Option<MediaQuery>,
}

/// One `@import` — `crate::parser`'s job is only to recognize and extract
/// this, not resolve/fetch it (no network access here); see the module
/// doc.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportRule {
    pub url: String,
    pub media: Option<MediaQuery>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    pub imports: Vec<ImportRule>,
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

    /// Parses `@import`'s body, already past the `AtKeyword` — either
    /// `"url"` or `url("url")` (only the quoted form; see the lexer's own
    /// "no `url()` token" note), then an optional trailing `@media`-shaped
    /// condition before the terminating `;`. Any tokens between the
    /// (optional) media query and `;` this doesn't recognize are consumed
    /// without failing the whole import, matching this parser's general
    /// "best-effort, don't corrupt the rest of the sheet" stance.
    fn parse_import(&mut self) -> Option<ImportRule> {
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
    fn parse_media_block(&mut self, rules: &mut Vec<Rule>) {
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
    fn parse_media_query(&mut self) -> Option<MediaQuery> {
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
