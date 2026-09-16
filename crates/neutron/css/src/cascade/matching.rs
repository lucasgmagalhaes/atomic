//! [`MatchedDeclarations`] and [`matching_declarations`] — the
//! unindexed "check every rule" cascade path, split out from `cascade.rs`.

use crate::parser::{Declaration, Stylesheet};

use super::snapshot::{selector_matches, ElementSnapshot};

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
