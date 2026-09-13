//! Real dirty-style-propagation coverage (`ROADMAP.md` item 11,
//! `.claude/plans/architecture-p5-foundations.plan.md` Stage 2):
//! `Dom::drain_style_invalidations` must name exactly which node(s) a
//! mutation affects, distinguish a subtree-wide invalidation from a
//! single-node one, and — the actual bug this item exists to prevent —
//! never report a sibling subtree as dirty just because an unrelated one
//! changed.

use dom::Dom;

#[test]
fn attribute_change_marks_only_that_node_as_a_subtree_invalidation() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    dom.append_child(root, el);
    dom.drain_style_invalidations(); // discard the append's own invalidations

    dom.set_attribute(el, "class", "highlighted");

    let invalidations = dom.drain_style_invalidations();
    assert_eq!(invalidations.len(), 1);
    assert_eq!(invalidations[0].root, el);
    assert!(
        invalidations[0].subtree,
        "an attribute change must invalidate el's own subtree (inherited properties, descendant combinators)"
    );
}

#[test]
fn a_mutation_in_one_subtree_does_not_dirty_an_unrelated_sibling_subtree() {
    let mut dom = Dom::new();
    let root = dom.root();
    let subtree_a = dom.create_element("div");
    let subtree_b = dom.create_element("div");
    dom.append_child(root, subtree_a);
    dom.append_child(root, subtree_b);
    dom.drain_style_invalidations();

    dom.set_attribute(subtree_a, "class", "x");

    let invalidations = dom.drain_style_invalidations();
    assert!(
        invalidations.iter().all(|inv| inv.root != subtree_b),
        "mutating subtree_a must never report subtree_b as needing re-cascade"
    );
    assert!(invalidations.iter().any(|inv| inv.root == subtree_a));
}

#[test]
fn hover_change_is_a_non_propagating_invalidation_on_exactly_the_affected_nodes() {
    let mut dom = Dom::new();
    let root = dom.root();
    let a = dom.create_element("div");
    let b = dom.create_element("div");
    dom.append_child(root, a);
    dom.append_child(root, b);
    dom.drain_style_invalidations();

    dom.set_hovered(a);
    let invalidations = dom.drain_style_invalidations();
    assert_eq!(invalidations.len(), 1);
    assert_eq!(invalidations[0].root, a);
    assert!(
        !invalidations[0].subtree,
        ":hover only affects the hovered element's own matched rules in this engine's scope, not its descendants"
    );

    // Moving hover from a to b must invalidate both — a's :hover no longer
    // matches, b's now does.
    dom.set_hovered(b);
    let invalidations = dom.drain_style_invalidations();
    assert_eq!(invalidations.len(), 2);
    assert!(invalidations
        .iter()
        .any(|inv| inv.root == a && !inv.subtree));
    assert!(invalidations
        .iter()
        .any(|inv| inv.root == b && !inv.subtree));
}

#[test]
fn focus_change_pushes_a_non_propagating_invalidation() {
    let mut dom = Dom::new();
    let root = dom.root();
    let input = dom.create_element("input");
    dom.append_child(root, input);
    dom.drain_style_invalidations();

    dom.focus(input);
    let invalidations = dom.drain_style_invalidations();
    assert_eq!(invalidations.len(), 1);
    assert_eq!(invalidations[0].root, input);
    assert!(!invalidations[0].subtree);

    dom.blur(input);
    let invalidations = dom.drain_style_invalidations();
    assert_eq!(invalidations.len(), 1);
    assert_eq!(invalidations[0].root, input);
    assert!(!invalidations[0].subtree);
}

#[test]
fn drain_resets_the_queue() {
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    dom.append_child(root, el);
    dom.set_attribute(el, "class", "x");

    assert!(!dom.drain_style_invalidations().is_empty());
    assert!(
        dom.drain_style_invalidations().is_empty(),
        "a second drain with no mutations in between must be empty"
    );
}

#[test]
fn appending_a_child_invalidates_the_parent_subtree_not_unrelated_nodes() {
    let mut dom = Dom::new();
    let root = dom.root();
    let parent = dom.create_element("ul");
    let unrelated = dom.create_element("div");
    dom.append_child(root, parent);
    dom.append_child(root, unrelated);
    dom.drain_style_invalidations();

    let child = dom.create_element("li");
    dom.append_child(parent, child);

    let invalidations = dom.drain_style_invalidations();
    assert!(invalidations
        .iter()
        .any(|inv| inv.root == parent && inv.subtree));
    assert!(
        invalidations.iter().all(|inv| inv.root != unrelated),
        "appending under parent must never report the unrelated sibling as dirty"
    );
}

#[test]
fn attribute_and_structural_mutations_do_not_touch_the_hover_focus_style_version() {
    // style_version is reserved for :hover/:focus state changes (see its
    // own field doc) — the new per-subtree queue is additive, not a
    // replacement, and must not start bumping this narrower counter for
    // mutations that already have their own signal (DirtyFlags::STYLE via
    // mark_dirty, drained separately).
    let mut dom = Dom::new();
    let root = dom.root();
    let el = dom.create_element("div");
    dom.append_child(root, el);
    let before = dom.style_version();

    dom.set_attribute(el, "class", "x");
    assert_eq!(dom.style_version(), before);

    let el2 = dom.create_element("span");
    dom.append_child(root, el2);
    assert_eq!(dom.style_version(), before);
}
