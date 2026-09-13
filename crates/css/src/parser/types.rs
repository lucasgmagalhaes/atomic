//! Selector/declaration/rule/stylesheet types — split out from
//! `parser/mod.rs`. See that file's module doc for the full grammar these
//! types model.

use crate::lexer::Token;

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
    MinHeight(f64),
    MaxHeight(f64),
    Height(f64),
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
    pub fn matches(&self, viewport_width: f64, viewport_height: f64) -> bool {
        self.type_matches
            && self.features.iter().all(|f| match f {
                MediaFeature::MinWidth(w) => viewport_width >= *w,
                MediaFeature::MaxWidth(w) => viewport_width <= *w,
                MediaFeature::Width(w) => (viewport_width - w).abs() < 0.001,
                MediaFeature::MinHeight(h) => viewport_height >= *h,
                MediaFeature::MaxHeight(h) => viewport_height <= *h,
                MediaFeature::Height(h) => (viewport_height - h).abs() < 0.001,
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
