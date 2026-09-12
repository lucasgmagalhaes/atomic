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
use std::collections::HashMap;

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
    /// Real live `:hover` state now — whether this is the element a
    /// caller's own hit-test currently reports as hovered (see
    /// `dom::Dom::hovered_element`). Defaults to `false` via `Default`,
    /// same "a caller that doesn't populate it just never matches"
    /// convention every other field here already documents — a caller
    /// with no notion of hover (e.g. a headless test) simply never sets
    /// this and `:hover` stays inert for it, not a panic.
    pub is_hovered: bool,
    /// Real live `:focus` state — whether this is `dom::Dom::active_element()`.
    /// Same default-`false`/opt-in convention as `is_hovered`.
    pub is_focused: bool,
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
            // Real now — see ElementSnapshot::is_hovered/is_focused's own
            // doc. A caller that never populates them (still valid, see
            // the field docs) gets the same "always false" behavior this
            // used to hard-code for everyone.
            PseudoClass::Hover => el.is_hovered,
            PseudoClass::Focus => el.is_focused,
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
/// `@media` condition (if any) matches `viewport_width`/`viewport_height`,
/// ordered lowest-to-highest cascade priority (specificity, then source
/// order) — apply in this order and let later ones overwrite earlier ones
/// per property to get the correct cascaded result. A rule inside an
/// `@media` block that doesn't match is skipped before selector matching
/// even runs, same as a real engine's media-query gate.
pub fn matching_declarations<'a>(
    sheet: &'a Stylesheet,
    chain: &[ElementSnapshot],
    viewport_width: f64,
    viewport_height: f64,
) -> Vec<MatchedDeclarations<'a>> {
    let mut matches = Vec::new();
    for (order, rule) in sheet.rules.iter().enumerate() {
        if let Some(media) = &rule.media {
            if !media.matches(viewport_width, viewport_height) {
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

/// A real (if narrow) selector-matching cache — bucketed by each
/// selector's *rightmost* compound (the one that must match the target
/// element itself, see [`selector_matches`]'s own doc for why that's the
/// end `compound_matches` checks first). Exactly one bucket per selector,
/// chosen by priority `id` > `class` > `tag` > "universal" (a compound
/// with no id/class/type simple selector — `*`, an attribute-only or
/// pseudo-class-only compound) — the same bucketing strategy real
/// browser engines' rule-set indices use. This turns
/// [`matching_declarations`]'s "check every rule against every element"
/// (`O(rules × elements)`) into "check only the rules whose rightmost
/// compound could possibly match this element's id/classes/tag" for the
/// overwhelming majority of stylesheets, where most rules key off a
/// specific id/class/tag rather than matching everything.
///
/// Built once per [`Stylesheet`] via [`build_selector_index`] and reused
/// across every element [`matching_declarations_indexed`] is called for
/// during one layout pass — the stylesheet itself doesn't change
/// mid-pass, so nothing here needs invalidation beyond rebuilding when
/// the stylesheet itself changes (a new `<style>`/`@import` is parsed).
#[derive(Debug, Default)]
pub struct SelectorIndex {
    by_id: HashMap<String, Vec<(usize, usize)>>,
    by_class: HashMap<String, Vec<(usize, usize)>>,
    by_tag: HashMap<String, Vec<(usize, usize)>>,
    /// Selectors whose rightmost compound has no id/class/type to key on
    /// (`*`, attribute-only, pseudo-class-only) — checked against every
    /// element, same as before indexing existed; real stylesheets rarely
    /// lean on these, so this bucket staying small is what makes the
    /// other three buckets an actual win rather than just moved cost.
    universal: Vec<(usize, usize)>,
}

/// Which bucket a selector's rightmost compound belongs in — see
/// [`SelectorIndex`]'s own doc for the id > class > tag > universal
/// priority.
enum BucketKey<'a> {
    Id(&'a str),
    Class(&'a str),
    Tag(&'a str),
    Universal,
}

fn bucket_key(compound: &crate::parser::CompoundSelector) -> BucketKey<'_> {
    for simple in &compound.0 {
        if let SimpleSelector::Id(name) = simple {
            return BucketKey::Id(name);
        }
    }
    for simple in &compound.0 {
        if let SimpleSelector::Class(name) = simple {
            return BucketKey::Class(name);
        }
    }
    for simple in &compound.0 {
        if let SimpleSelector::Type(name) = simple {
            return BucketKey::Tag(name);
        }
    }
    BucketKey::Universal
}

/// Builds a [`SelectorIndex`] for `sheet` — one pass over every rule's
/// every selector, keying each by its rightmost compound (see
/// [`bucket_key`]). Cheap relative to the many-element matching passes
/// it then accelerates; call once per stylesheet (e.g. once per
/// `build_box_tree` call in `layout-engine`), not once per element.
pub fn build_selector_index(sheet: &Stylesheet) -> SelectorIndex {
    let mut index = SelectorIndex::default();
    for (rule_idx, rule) in sheet.rules.iter().enumerate() {
        for (selector_idx, selector) in rule.selectors.0.iter().enumerate() {
            let Some(rightmost) = selector.0.last() else {
                continue;
            };
            match bucket_key(rightmost) {
                BucketKey::Id(name) => index
                    .by_id
                    .entry(name.to_string())
                    .or_default()
                    .push((rule_idx, selector_idx)),
                BucketKey::Class(name) => index
                    .by_class
                    .entry(name.to_string())
                    .or_default()
                    .push((rule_idx, selector_idx)),
                BucketKey::Tag(name) => index
                    .by_tag
                    .entry(name.to_string())
                    .or_default()
                    .push((rule_idx, selector_idx)),
                BucketKey::Universal => index.universal.push((rule_idx, selector_idx)),
            }
        }
    }
    index
}

/// Same result as [`matching_declarations`] (every declaration block
/// whose selector matches `chain` and whose `@media` condition matches,
/// cascade-ordered), but only checks the candidate selectors
/// [`SelectorIndex`] narrows `chain`'s target element down to instead of
/// every selector in the stylesheet. A rule matched by more than one of
/// its own selectors (a comma-separated selector list) still contributes
/// one entry at its highest matching specificity, same as
/// `matching_declarations`'s own per-rule `max()`.
pub fn matching_declarations_indexed<'a>(
    index: &SelectorIndex,
    sheet: &'a Stylesheet,
    chain: &[ElementSnapshot],
    viewport_width: f64,
    viewport_height: f64,
) -> Vec<MatchedDeclarations<'a>> {
    let Some(target) = chain.last() else {
        return Vec::new();
    };
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    if let Some(id) = target.id {
        if let Some(v) = index.by_id.get(id) {
            candidates.extend(v.iter().copied());
        }
    }
    for class in &target.classes {
        if let Some(v) = index.by_class.get(*class) {
            candidates.extend(v.iter().copied());
        }
    }
    if let Some(v) = index.by_tag.get(target.tag) {
        candidates.extend(v.iter().copied());
    }
    candidates.extend(index.universal.iter().copied());

    let mut best_by_rule: HashMap<usize, (u32, u32, u32)> = HashMap::new();
    for (rule_idx, selector_idx) in candidates {
        let rule = &sheet.rules[rule_idx];
        if let Some(media) = &rule.media {
            if !media.matches(viewport_width, viewport_height) {
                continue;
            }
        }
        let Some(selector) = rule.selectors.0.get(selector_idx) else {
            continue;
        };
        if selector_matches(selector, chain) {
            let spec = selector.specificity();
            best_by_rule
                .entry(rule_idx)
                .and_modify(|existing| {
                    if spec > *existing {
                        *existing = spec;
                    }
                })
                .or_insert(spec);
        }
    }

    let mut matches: Vec<MatchedDeclarations> = best_by_rule
        .into_iter()
        .map(|(rule_idx, specificity)| MatchedDeclarations {
            specificity,
            source_order: rule_idx,
            declarations: &sheet.rules[rule_idx].declarations,
        })
        .collect();
    matches.sort_by_key(|m| (m.specificity, m.source_order));
    matches
}
