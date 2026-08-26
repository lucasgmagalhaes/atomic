//! Selector matching + cascade ordering. Decoupled from `dom` on purpose —
//! takes a plain ancestor-chain snapshot rather than a live tree, so this
//! crate doesn't need to depend on `dom`'s node representation. Whatever
//! wires CSS into layout builds the snapshot from a real `dom::Dom` walk.
//!
//! [`ElementSnapshot`] also carries `attributes` and sibling-position data
//! (`preceding_siblings`/`has_following_sibling`) now, for attribute
//! selectors, sibling combinators (`+`/`~`), and structural pseudo-classes
//! (`:first-child`/`:last-child`/`:nth-child`) — a caller that doesn't
//! populate them (they default to empty/`false` via `Default`) simply
//! never matches those selector kinds, rather than panicking; that's a
//! real, honest limitation for a caller that hasn't been updated yet, not
//! fake behavior.
use crate::parser::{
    Combinator, ComplexSelector, Declaration, PseudoClass, SimpleSelector, Stylesheet,
};

/// Borrows every field from the live `dom::Node`/attribute map a caller
/// (`layout-engine`'s `tree.rs`, `js-runtime`'s `dom_bindings/selectors.rs`)
/// already holds a reference to, rather than cloning each `String` per
/// element per selector-matching pass — this snapshot is rebuilt on every
/// layout pass and every `querySelector`/`matches`/`closest` call, so an
/// owned copy here was real, measurable per-element allocation pressure
/// (see `spec/RULES.md`'s hot-path rule). `'a` is the borrowed DOM data's
/// own lifetime, threaded through recursively via `preceding_siblings`.
#[derive(Debug, Clone, Default)]
pub struct ElementSnapshot<'a> {
    pub tag: &'a str,
    pub id: Option<&'a str>,
    pub classes: Vec<&'a str>,
    pub attributes: Vec<(&'a str, &'a str)>,
    /// Element siblings under the same parent that come *before* this one,
    /// in document order (the immediately preceding sibling is last) —
    /// used by `+`/`~` and `:nth-child`/`:first-child`. Flat, not
    /// recursively nested: matching a chain like `a + b + c` only ever
    /// needs to walk *this* element's own preceding-sibling list, since
    /// every compound in such a chain refers to a sibling under the same
    /// parent.
    pub preceding_siblings: Vec<ElementSnapshot<'a>>,
    /// Whether at least one element sibling follows this one under the
    /// same parent — used by `:last-child`.
    pub has_following_sibling: bool,
}

fn compound_matches(compound: &crate::parser::CompoundSelector, el: &ElementSnapshot<'_>) -> bool {
    compound.0.iter().all(|simple| match simple {
        SimpleSelector::Universal => true,
        SimpleSelector::Type(name) => el.tag == name.as_str(),
        SimpleSelector::Id(id) => el.id == Some(id.as_str()),
        SimpleSelector::Class(class) => el.classes.iter().any(|c| *c == class.as_str()),
        SimpleSelector::Attribute(attr) => match &attr.match_ {
            crate::parser::AttributeMatch::Has => {
                el.attributes.iter().any(|(k, _)| *k == attr.name)
            }
            crate::parser::AttributeMatch::Equals(v) => el
                .attributes
                .iter()
                .any(|(k, val)| *k == attr.name && *val == v),
        },
        SimpleSelector::PseudoClass(pseudo) => match pseudo {
            PseudoClass::FirstChild => el.preceding_siblings.is_empty(),
            PseudoClass::LastChild => !el.has_following_sibling,
            PseudoClass::NthChild(formula) => {
                formula.matches(el.preceding_siblings.len() as i64 + 1)
            }
            // No real hover/focus state is tracked anywhere in this engine
            // yet (no input-event plumbing reaches selector matching) -
            // always false rather than faking a match. See the parser
            // module doc.
            PseudoClass::Hover | PseudoClass::Focus => false,
        },
    })
}

/// `chain` is the ancestor path from document root to the target element,
/// target element last (each entry's own `preceding_siblings` describing
/// *its* siblings, not the target's - populated per-element by the
/// caller). Matches the rightmost compound against the target, then walks
/// the remaining compounds right-to-left per their [`Combinator`]:
/// descendant/child consume ancestors from `chain`; next-/subsequent-
/// sibling consume from a separate `remaining_siblings` cursor, reset to
/// "current element"'s own `preceding_siblings` whenever an ancestor
/// combinator switches which element is current, but otherwise carried
/// forward unchanged across consecutive sibling combinators (`a + b + c`
/// all refer to siblings under the *same* parent, so `c`'s combinator
/// consumes `b` from the end of the list and `b`'s combinator must then
/// keep consuming from what's left of that *same* list, not `b`'s own
/// - likely empty - `preceding_siblings`).
pub fn selector_matches(selector: &ComplexSelector, chain: &[ElementSnapshot]) -> bool {
    let compounds = &selector.0;
    let combinators = &selector.1;
    if compounds.is_empty() {
        return false;
    }
    let Some((target_el, ancestor_els)) = chain.split_last() else {
        return false;
    };
    if !compound_matches(&compounds[compounds.len() - 1], target_el) {
        return false;
    }

    let mut remaining_ancestors = ancestor_els;
    let mut remaining_siblings = target_el.preceding_siblings.as_slice();

    for i in (0..compounds.len() - 1).rev() {
        let compound = &compounds[i];
        match combinators[i] {
            Combinator::Descendant => loop {
                let Some((candidate, rest)) = remaining_ancestors.split_last() else {
                    return false; // ran out of ancestors before matching every compound
                };
                remaining_ancestors = rest;
                if compound_matches(compound, candidate) {
                    remaining_siblings = candidate.preceding_siblings.as_slice();
                    break;
                }
            },
            Combinator::Child => {
                let Some((candidate, rest)) = remaining_ancestors.split_last() else {
                    return false;
                };
                if !compound_matches(compound, candidate) {
                    return false;
                }
                remaining_ancestors = rest;
                remaining_siblings = candidate.preceding_siblings.as_slice();
            }
            Combinator::NextSibling => {
                let Some((candidate, rest)) = remaining_siblings.split_last() else {
                    return false;
                };
                if !compound_matches(compound, candidate) {
                    return false;
                }
                remaining_siblings = rest;
            }
            Combinator::SubsequentSibling => {
                let Some(idx) = remaining_siblings
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, c)| compound_matches(compound, c))
                    .map(|(idx, _)| idx)
                else {
                    return false;
                };
                remaining_siblings = &remaining_siblings[..idx];
            }
        }
    }
    true
}

/// A rule's declarations paired with the specificity/order it should be
/// applied at, for a specific matched selector within that rule.
pub struct MatchedDeclarations<'a> {
    pub specificity: (u32, u32, u32),
    pub source_order: usize,
    pub declarations: &'a [Declaration],
}

/// Every declaration block whose selector matches `chain` *and* whose
/// `@media` condition (if any) matches `viewport_width`, ordered
/// lowest-to-highest cascade priority (specificity, then source order) —
/// apply in this order and let later ones overwrite earlier ones per
/// property to get the correct cascaded result. A rule inside an `@media`
/// block that doesn't match `viewport_width` is skipped before selector
/// matching even runs, same as a real engine's media-query gate.
pub fn matching_declarations<'a>(
    sheet: &'a Stylesheet,
    chain: &[ElementSnapshot],
    viewport_width: f64,
) -> Vec<MatchedDeclarations<'a>> {
    let mut matches = Vec::new();
    for (order, rule) in sheet.rules.iter().enumerate() {
        if let Some(media) = &rule.media {
            if !media.matches(viewport_width) {
                continue;
            }
        }
        let best = rule
            .selectors
            .0
            .iter()
            .filter(|s| selector_matches(s, chain))
            .map(|s| s.specificity())
            .max();
        if let Some(specificity) = best {
            matches.push(MatchedDeclarations {
                specificity,
                source_order: order,
                declarations: &rule.declarations,
            });
        }
    }
    matches.sort_by_key(|m| (m.specificity, m.source_order));
    matches
}
