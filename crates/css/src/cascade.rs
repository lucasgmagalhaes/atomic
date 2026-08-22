//! Selector matching + cascade ordering. Decoupled from `dom` on purpose —
//! takes a plain ancestor-chain snapshot rather than a live tree, so this
//! crate doesn't need to depend on `dom`'s node representation. Whatever
//! wires CSS into layout builds the snapshot from a real `dom::Dom` walk.
use crate::parser::{ComplexSelector, Declaration, SimpleSelector, Stylesheet};

#[derive(Debug, Clone)]
pub struct ElementSnapshot {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

fn compound_matches(compound: &crate::parser::CompoundSelector, el: &ElementSnapshot) -> bool {
    compound.0.iter().all(|simple| match simple {
        SimpleSelector::Universal => true,
        SimpleSelector::Type(name) => el.tag == *name,
        SimpleSelector::Id(id) => el.id.as_deref() == Some(id.as_str()),
        SimpleSelector::Class(class) => el.classes.iter().any(|c| c == class),
    })
}

/// `chain` is the ancestor path from document root to the target element,
/// target element last. Matches the rightmost compound against the target,
/// then each earlier compound against *some* ancestor further up the chain
/// (descendant combinator semantics - not required to be the immediate
/// parent, since this crate only supports `>`-free selectors).
pub fn selector_matches(selector: &ComplexSelector, chain: &[ElementSnapshot]) -> bool {
    let Some((target_selector, ancestor_selectors)) = selector.0.split_last() else {
        return false;
    };
    let Some((target_el, ancestor_els)) = chain.split_last() else {
        return false;
    };
    if !compound_matches(target_selector, target_el) {
        return false;
    }

    // Walk the remaining selector compounds right-to-left, consuming as
    // many ancestors as needed for each one to find a match.
    let mut remaining_ancestors = ancestor_els;
    for compound in ancestor_selectors.iter().rev() {
        loop {
            let Some((candidate, rest)) = remaining_ancestors.split_last() else {
                return false; // ran out of ancestors before matching every compound
            };
            remaining_ancestors = rest;
            if compound_matches(compound, candidate) {
                break;
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

/// Every declaration block whose selector matches `chain`, ordered
/// lowest-to-highest cascade priority (specificity, then source order) —
/// apply in this order and let later ones overwrite earlier ones per
/// property to get the correct cascaded result.
pub fn matching_declarations<'a>(
    sheet: &'a Stylesheet,
    chain: &[ElementSnapshot],
) -> Vec<MatchedDeclarations<'a>> {
    let mut matches = Vec::new();
    for (order, rule) in sheet.rules.iter().enumerate() {
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
