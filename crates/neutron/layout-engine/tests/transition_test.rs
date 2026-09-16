//! Real CSS transitions (`transition-property`/`transition-duration`) —
//! see `crate::transition`'s own doc for the exact "opacity/transform
//! only, linear, no delay" scope cut. `TransitionStates::apply` is a pure
//! `(tree, now) -> mutated tree` step, so every test below drives it with
//! explicit `Instant`s instead of real `sleep`s - deterministic and fast.

use std::time::{Duration, Instant};

use css::parse_stylesheet;
use dom::Dom;
use layout_engine::style::TransitionProperty;
use layout_engine::{build_box_tree, layout_block, TransitionStates};

fn build_single_div() -> (Dom, dom::NodeId) {
    let mut d = Dom::new();
    let root = d.root();
    let div = d.create_element("div");
    d.set_attribute(div, "id", "box");
    d.append_child(root, div);
    (d, div)
}

#[test]
fn a_transitioning_propertys_first_paint_never_animates() {
    let (d, container) = build_single_div();
    let sheet = parse_stylesheet("#box { transition: opacity 1s; opacity: 0.5; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let mut states = TransitionStates::new();
    let now = Instant::now();
    let active = states.apply(&mut tree, now);

    assert!(active.is_empty());
    assert_eq!(tree.style.opacity, 0.5);
}

#[test]
fn a_later_value_change_animates_linearly_from_the_old_value() {
    let (d, container) = build_single_div();
    let sheet = parse_stylesheet("#box { transition: opacity 1s; opacity: 1.0; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let mut states = TransitionStates::new();
    let start = Instant::now();
    states.apply(&mut tree, start); // settles at 1.0, no animation.

    // A real style change: the cascade now resolves opacity to 0.0.
    tree.style.opacity = 0.0;
    let active = states.apply(&mut tree, start);
    assert_eq!(active.len(), 1);
    // At t=0 into the transition, the box is still at its old value.
    assert_eq!(tree.style.opacity, 1.0);

    tree.style.opacity = 0.0;
    let active = states.apply(&mut tree, start + Duration::from_millis(500));
    assert_eq!(active.len(), 1);
    assert!((tree.style.opacity - 0.5).abs() < 1e-9);

    tree.style.opacity = 0.0;
    let active = states.apply(&mut tree, start + Duration::from_secs(1));
    assert!(active.is_empty());
    assert_eq!(tree.style.opacity, 0.0);
}

#[test]
fn retargeting_mid_flight_starts_from_the_current_interpolated_value() {
    let (d, container) = build_single_div();
    let sheet = parse_stylesheet("#box { transition: opacity 1s; opacity: 0.0; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let mut states = TransitionStates::new();
    let start = Instant::now();
    states.apply(&mut tree, start); // settles at 0.0.

    tree.style.opacity = 1.0;
    states.apply(&mut tree, start); // now animating 0.0 -> 1.0.

    // Halfway there (0.5), the target changes again before it finishes.
    tree.style.opacity = 1.0;
    states.apply(&mut tree, start + Duration::from_millis(500));

    tree.style.opacity = 0.0;
    let active = states.apply(&mut tree, start + Duration::from_millis(500));
    assert_eq!(active.len(), 1);
    // Retargeted from wherever it currently sat (~0.5), not from 1.0 or
    // a jump back to 0.0.
    assert!((tree.style.opacity - 0.5).abs() < 1e-9);
}

#[test]
fn zero_duration_never_animates() {
    let (d, container) = build_single_div();
    let sheet = parse_stylesheet("#box { opacity: 1.0; }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);
    assert_eq!(tree.style.transition_duration, 0.0);

    let mut states = TransitionStates::new();
    let now = Instant::now();
    states.apply(&mut tree, now);
    tree.style.opacity = 0.0;
    let active = states.apply(&mut tree, now);

    assert!(active.is_empty());
    assert_eq!(tree.style.opacity, 0.0);
}

#[test]
fn transition_property_transform_animates_translate() {
    let (d, container) = build_single_div();
    let sheet =
        parse_stylesheet("#box { transition: transform 1s; transform: translate(0px, 0px); }");
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);
    assert_eq!(
        tree.style.transition_property,
        TransitionProperty::Transform
    );

    let mut states = TransitionStates::new();
    let start = Instant::now();
    states.apply(&mut tree, start); // settles at (0, 0).

    tree.style.transform = (100.0, 0.0);
    states.apply(&mut tree, start);
    tree.style.transform = (100.0, 0.0);
    let active = states.apply(&mut tree, start + Duration::from_millis(250));

    assert_eq!(active.len(), 1);
    assert!((tree.style.transform.0 - 25.0).abs() < 1e-9);
    assert_eq!(tree.style.transform.1, 0.0);
}

#[test]
fn transition_all_animates_both_opacity_and_transform_independently() {
    let (d, container) = build_single_div();
    let sheet = parse_stylesheet(
        "#box { transition: all 1s; opacity: 1.0; transform: translate(0px, 0px); }",
    );
    let mut tree = build_box_tree(&d, container, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);
    assert_eq!(tree.style.transition_property, TransitionProperty::All);

    let mut states = TransitionStates::new();
    let start = Instant::now();
    states.apply(&mut tree, start); // settles at 1.0 / (0, 0).

    tree.style.opacity = 0.0;
    tree.style.transform = (50.0, 10.0);
    states.apply(&mut tree, start); // registers the retarget at t=0.

    tree.style.opacity = 0.0;
    tree.style.transform = (50.0, 10.0);
    let active = states.apply(&mut tree, start + Duration::from_millis(500));

    assert_eq!(active.len(), 2);
    assert!((tree.style.opacity - 0.5).abs() < 1e-9);
    assert!((tree.style.transform.0 - 25.0).abs() < 1e-9);
    assert!((tree.style.transform.1 - 5.0).abs() < 1e-9);
}

#[test]
fn children_animate_independently_of_their_parent() {
    let mut d = Dom::new();
    let root = d.root();
    let parent = d.create_element("div");
    d.set_attribute(parent, "id", "parent");
    d.append_child(root, parent);
    let child = d.create_element("div");
    d.set_attribute(child, "id", "child");
    d.append_child(parent, child);

    let sheet = parse_stylesheet(
        "#parent { transition: opacity 1s; opacity: 1.0; } \
         #child { transition: opacity 1s; opacity: 1.0; }",
    );
    let mut tree = build_box_tree(&d, parent, &sheet).unwrap();
    layout_block(&mut tree, 800.0, 0.0, 0.0);

    let mut states = TransitionStates::new();
    let start = Instant::now();
    states.apply(&mut tree, start); // settles both at 1.0.

    // Only the parent's target changes this round.
    tree.style.opacity = 0.0;
    states.apply(&mut tree, start); // registers the parent's retarget at t=0.

    tree.style.opacity = 0.0;
    let active = states.apply(&mut tree, start + Duration::from_millis(500));

    assert_eq!(active.len(), 1);
    assert!((tree.style.opacity - 0.5).abs() < 1e-9);
    assert_eq!(tree.children[0].style.opacity, 1.0);
}
