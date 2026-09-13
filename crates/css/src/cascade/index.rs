//! [`SelectorIndex`], [`build_selector_index`], and
//! [`matching_declarations_indexed`] — the indexed, faster-than-linear
//! cascade path, split out from `cascade.rs`.

use std::collections::HashMap;

use crate::parser::{SimpleSelector, Stylesheet};

use super::matching::MatchedDeclarations;
use super::snapshot::{selector_matches, ElementSnapshot};

/// A real (if narrow) selector-matching cache — bucketed by each
/// selector's *rightmost* compound (the one that must match the target
/// element itself, see [`selector_matches`]'s own doc for why that's the
/// end `compound_matches` checks first). Exactly one bucket per selector,
/// chosen by priority `id` > `class` > `tag` > "universal" (a compound
/// with no id/class/type simple selector — `*`, an attribute-only or
/// pseudo-class-only compound) — the same bucketing strategy real
/// browser engines' rule-set indices use. This turns
/// [`matching_declarations`](super::matching_declarations)'s "check every
/// rule against every element" (`O(rules × elements)`) into "check only
/// the rules whose rightmost compound could possibly match this element's
/// id/classes/tag" for the overwhelming majority of stylesheets, where
/// most rules key off a specific id/class/tag rather than matching
/// everything.
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

/// Same result as [`matching_declarations`](super::matching_declarations)
/// (every declaration block whose selector matches `chain` and whose
/// `@media` condition matches, cascade-ordered), but only checks the
/// candidate selectors [`SelectorIndex`] narrows `chain`'s target element
/// down to instead of every selector in the stylesheet. A rule matched by
/// more than one of its own selectors (a comma-separated selector list)
/// still contributes one entry at its highest matching specificity, same
/// as `matching_declarations`'s own per-rule `max()`.
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
