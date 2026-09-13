//! Compositing layer cache primitive (`ROADMAP.md` item 28,
//! `.claude/plans/architecture-p5-foundations.plan.md` Stage 5) — no real
//! GPU compositing wired up yet, this only proves the cache-key shape
//! itself behaves like `profile-worker`'s own `PaintCache` (item 13): a
//! fresh layer is always invalid, storing pixels makes the matching key
//! valid, and a different key (or a different layer) never reuses them.

use dom::Dom;
use render::{Layer, LayerCacheKey};

fn key(seed: u64) -> LayerCacheKey {
    LayerCacheKey {
        width: 800,
        height: 600,
        layout_ver: seed,
        style_ver: seed,
        adopted_version: 0,
    }
}

#[test]
fn a_fresh_layer_is_never_valid_before_anything_is_stored() {
    let mut dom = Dom::new();
    let root = dom.create_element("div");
    let layer = Layer::new(root);
    assert!(!layer.is_valid(key(1)));
}

#[test]
fn storing_pixels_makes_the_matching_key_valid() {
    let mut dom = Dom::new();
    let root = dom.create_element("div");
    let mut layer = Layer::new(root);
    let k = key(1);
    layer.store(k, vec![1, 2, 3, 4]);
    assert!(layer.is_valid(k));
    assert_eq!(layer.pixels, vec![1, 2, 3, 4]);
}

#[test]
fn a_different_key_invalidates_the_cached_pixels() {
    let mut dom = Dom::new();
    let root = dom.create_element("div");
    let mut layer = Layer::new(root);
    layer.store(key(1), vec![1, 2, 3, 4]);
    assert!(!layer.is_valid(key(2)));
}

#[test]
fn two_layers_for_different_stacking_context_roots_are_independent() {
    let mut dom = Dom::new();
    let a = dom.create_element("div");
    let b = dom.create_element("div");
    let mut layer_a = Layer::new(a);
    let layer_b = Layer::new(b);

    layer_a.store(key(1), vec![9, 9, 9]);
    assert!(layer_a.is_valid(key(1)));
    // layer_b never had anything stored — a mutation that only affected
    // layer_a's subtree must not make layer_b look valid too.
    assert!(!layer_b.is_valid(key(1)));
    assert_ne!(layer_a.stacking_context_root, layer_b.stacking_context_root);
}
