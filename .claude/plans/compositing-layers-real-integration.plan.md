# Plan: Compositing Layers — Real Integration (ROADMAP item 28)

**Source**: `spec/ROADMAP.md` item 28 (no PRD — free-form continuation of `.claude/plans/architecture-p5-foundations.plan.md` Stage 5, which only landed the `render::Layer`/`LayerCacheKey` type shape)
**Complexity**: Large

## Summary

Wire `render::Layer` into `profile-worker`'s real `Page::render` pipeline: identify boxes that establish a real stacking context (`position != Static && z_index.is_some()`, `opacity < 1.0`, or a non-identity `transform` — all three already resolved in `layout_engine::ComputedStyle`), paint each as its own transparent-background canvas cached independently, and alpha-composite them onto the base frame in z-index order — replacing today's single whole-page opaque `render_to_rgba` pass.

The naive version of this (give every layer the same coarse cache key `PaintCache` already uses) would deliver **zero** real caching benefit: since every layer's key would invalidate simultaneously on any page-wide change, this pass also wires Stage 2's already-built `dom::StyleInvalidation` queue (`drain_style_invalidations`) to give each layer its own real per-subtree dirty signal — the actual reason item 28 exists ("where performance benefits justify it").

## Patterns to Mirror

| Category | Source | Pattern |
|---|---|---|
| Cache-key-and-skip | `crates/profile/src/bin/profile_worker/page/mod.rs`'s `LayoutCache`/`PaintCache` | A cache struct holding the exact inputs it was computed from; `render()` compares before recomputing, reuses on a hit. `Layer`/`LayerCacheKey` (`crates/render/src/layer.rs`) already generalize this shape per-subtree; this plan is about actually calling into it. |
| Alpha compositing | `crates/render/src/image.rs`'s `blend_over`/`composite_images`, `crates/render/src/text.rs`'s own copy | Small, local, non-shared `blend_over(dst: &mut [u8], src: [u8;4])` per file — this plan adds a third copy for whole-layer-canvas compositing, matching the existing "no shared utility, small per-file copies" convention already established between `image.rs`/`text.rs`. |
| Transparent-canvas rendering | `crates/render/src/gpu/render.rs`'s `render_to_rgba(rects, w, h, clear_color)` + `crates/render/src/gpu/construct.rs`'s `wgpu::BlendState::ALPHA_BLENDING` (already configured on the pipeline) | Passing `clear_color: [0.0, 0.0, 0.0, 0.0]` instead of the page's opaque background gives a real transparent per-layer canvas — the GPU pipeline already blends correctly onto it, no shader change needed. |
| Per-subtree invalidation | `crates/dom/src/lib.rs`'s `StyleInvalidation { root, subtree }` / `Dom::drain_style_invalidations` (Stage 2, already landed) | Exactly the signal a real per-layer cache needs: which subtree(s) actually changed since last drain. `Dom::contains(ancestor, other)` (already `pub`) is the exact primitive to test "does invalidation `X` fall under layer root `R`". |
| Tests | `crates/profile/tests/paint_cache_test.rs` (item 13's own tests: real DOM mutation invalidates, unchanged-page reuses, focus-only invalidates) | Same three-shape test set, retargeted at layers: an out-of-layer mutation must NOT invalidate an unrelated layer's cache; an in-layer mutation must. |

## Files to Change

| File | Action | Why |
|---|---|---|
| `crates/render/src/layer.rs` | UPDATE | `Layer::is_valid`/`store` already exist; no field changes needed, but the module doc's "no render integration" framing needs updating once this lands. |
| `crates/render/src/compositor.rs` | CREATE | New `composite_layer_onto(base: &mut [u8], layer: &[u8], width, height)` — the missing alpha-blend-a-whole-canvas-onto-another-canvas primitive (today `composite_images`/`composite_glyphs` blend individual primitives, not whole pre-rendered canvases). |
| `crates/render/src/lib.rs` | UPDATE | `pub mod compositor;` + re-export. |
| `crates/layout-engine/src/tree/types.rs` or a new `crates/layout-engine/src/stacking.rs` | CREATE/UPDATE | A real `fn establishes_stacking_context(style: &ComputedStyle) -> bool` predicate (the three-condition check) and `fn find_layer_roots(tree: &LayoutBox) -> Vec<&LayoutBox>` (non-recursing-into-found-roots walk) — small, pure, easily unit-testable in `layout-engine` itself rather than duplicated logic inside `profile`. |
| `crates/profile/src/bin/profile_worker/page/mod.rs` | UPDATE | `Page` gains `layers: RefCell<HashMap<dom::NodeId, LayerCacheEntry>>` (persistent across frames) and `style_epoch_seen: Cell<u64>`-shaped bookkeeping for draining `dom::StyleInvalidation`s once per render and mapping them onto layer roots. |
| `crates/profile/src/bin/profile_worker/page/render.rs` | UPDATE | `render()`'s recompute path: prune layer-root subtrees out of the base tree before its own `build_display_list`/`build_image_list`/`build_glyph_list` calls; for each layer root, check per-layer dirtiness (from drained invalidations) before recomputing; composite layers onto the base canvas in z-index order via the new `compositor::composite_layer_onto`. |
| `crates/profile/tests/paint_cache_test.rs` or a new `crates/profile/tests/compositing_layer_test.rs` | CREATE | Real coverage: a layer (e.g. `position: relative; z-index: 1`) renders correctly on top of base content; mutating *inside* the layer invalidates only that layer's cache (base reused); mutating *outside* the layer invalidates only the base (layer reused); z-index ordering across two layers paints correctly. |
| `spec/ROADMAP.md`, `.claude/plans/architecture-p5-foundations.plan.md` | UPDATE | Flip item 28 to `[~]` with the real scope note once landed. |

## Tasks

### Task 1: Stacking-context predicate + layer-root finder (layout-engine)
- **Action**: Add `establishes_stacking_context(style: &ComputedStyle) -> bool` (`position != Position::Static && z_index.is_some()`, OR `opacity < 1.0`, OR `transform != (0.0, 0.0)`) and `find_layer_roots<'a>(tree: &'a LayoutBox) -> Vec<&'a LayoutBox>` — a pre-order walk that stops descending once a box matches (nested stacking contexts stay part of their nearest ancestor layer, a documented scope cut: no nested/independently-cacheable layers this pass).
- **Mirror**: Pure, standalone functions with their own doc comment explaining the exact three-condition spec approximation and the nesting scope cut, same density as `Overflow`/`Float`'s own scope-cut docs in `style/types.rs`.
- **Validate**: `cargo test -p layout-engine --release -- --test-threads=1` (new unit tests: a plain box is never a layer root; `position: relative; z-index: 1` is; a layer root's own descendants are excluded from the returned list even if they'd otherwise qualify).

### Task 2: Whole-canvas alpha compositing (render)
- **Action**: `compositor::composite_layer_onto(base: &mut [u8], layer: &[u8], width, height)` — per-pixel `src-over` blend of two same-sized RGBA8 buffers (mirrors `image.rs`'s `blend_over`, just applied to a whole buffer instead of per-quad-per-pixel).
- **Mirror**: `image.rs`'s own `blend_over` formula, copied not shared (per this crate's established convention).
- **Validate**: `cargo test -p render --release -- --test-threads=1` (a transparent layer changes nothing; an opaque layer fully replaces; a half-alpha layer blends the expected midpoint).

### Task 3: Per-layer persistent cache + style-invalidation-driven dirtiness (profile-worker)
- **Action**: `Page` gains a `layers: RefCell<HashMap<dom::NodeId, LayerCacheEntry>>` (keyed by each layer root's own `NodeId`, persisting across frames — a `Layer`'s cached pixels must survive to the *next* `render()` call, unlike the current single `paint_cache` which is fine being fully rebuilt each miss since there's only one). Each `render()` call that reaches the recompute path (i.e., the existing whole-frame `PaintCache` missed) drains `dom.drain_style_invalidations()` once and, for each `StyleInvalidation { root, subtree }`, marks every *currently known* layer whose own root either equals `root` or (`subtree` is `true` and) contains `root` as dirty-this-frame (via `dom.contains(layer_root, invalidation.root)`); a layer not marked dirty AND whose `width`/`height`/`adopted_version` still match keeps its cached pixels — real, viewport/stylesheet-level changes still invalidate every layer coarsely (a further, documented scope cut: this pass has no per-layer viewport/stylesheet tracking, matching how `LayerCacheKey`'s own doc already flags this as coarser than a true per-layer damage signal).
- **Mirror**: `LayoutCache`/`PaintCache`'s own "key includes exactly the inputs that can invalidate it" shape, refined per-layer using Stage 2's `StyleInvalidation` instead of the page-wide `layout_ver`/`style_ver` pair.
- **Validate**: `cargo test -p profile --release -- --test-threads=1` (the new `compositing_layer_test.rs`'s in-layer-vs-out-of-layer invalidation cases are the real proof this isn't just item 13 rebranded).

### Task 4: Wire the recompute path in `Page::render`
- **Action**: On a whole-frame `PaintCache` miss: run `layout()` as today, then `find_layer_roots(&tree)`; build a pruned clone of the tree (each layer-root subtree replaced by an empty placeholder box at the same `dimensions`/`node`, so the base pass's own `build_display_list`/`build_image_list`/`build_glyph_list` skip its content entirely); render the base canvas exactly as today (opaque background) from the pruned tree; for each layer root (sorted by `z_index.unwrap_or(0)` ascending, ties in document order), reuse or recompute its own transparent-canvas bitmap (its own `build_display_list`/`build_image_list`/`build_glyph_list` called with that sub-box as the root, `render_to_rgba` cleared to `[0,0,0,0]`), then `compositor::composite_layer_onto` it onto the base canvas. Finally still populate the existing whole-frame `PaintCache` with the fully composited result (item 13's outermost "did literally nothing change" bypass stays the fastest path, untouched).
- **Mirror**: The existing `rects`/`images`/`glyphs` shift-by-`scroll_top` logic already in `render()` — each layer's own canvas needs the identical shift applied to its own local `build_*` calls, not a re-derived offset.
- **Validate**: `cargo build --workspace --exclude shell` + full `compositing_layer_test.rs` suite + a manual visual check is not feasible headlessly, so correctness rests on pixel-level assertions in the new test (e.g. a known-color layer rect's exact RGBA at a known coordinate in the final composited output).

## Validation

```bash
cargo test -p layout-engine -p render -p profile --release -- --test-threads=1
cargo build --workspace --exclude shell
cargo test --workspace --exclude shell --exclude automation --release -- --test-threads=1
cargo fmt --all
```

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| GPU alpha blending onto a transparent-cleared canvas doesn't compose bit-for-bit identically to the opaque path (premultiplied vs straight alpha mismatches) | Medium | `wgpu::BlendState::ALPHA_BLENDING` is already the configured pipeline blend state (confirmed via `gpu/construct.rs`) — no shader change needed, but Task 2's own tests must assert exact byte values, not just "looks right", to catch a premultiplication mismatch early. |
| Pruning layer-root subtrees from the base tree by matching `NodeId` could misfire if a `NodeId` isn't unique in the tree (an inline-merged box's `node` field "identifies only its first source child" per `LayoutBox::node`'s own doc) | Low | Layer roots are found by walking the *unpruned* tree once and capturing exact tree positions (index paths), not by a `NodeId`-based post-hoc search — avoids the ambiguity entirely. |
| Persistent per-`NodeId`-keyed layer cache (`Page::layers`) can grow unboundedly if layer roots come and go across many navigations/mutations without ever being pruned | Medium | Prune `Page::layers` entries whose `NodeId` no longer resolves to a layer root in the current `find_layer_roots(&tree)` result, once per recompute — cheap (already walking the fresh list) and keeps the map bounded by "layers that exist right now." |
| This pass's per-layer key is still coarser than a true damage-rect system (whole-layer-or-nothing, no per-primitive damage) | High (accepted) | Documented scope cut, consistent with `LayerCacheKey`'s own existing doc ("still strictly more precise than one whole-frame cache") — a full damage-rectangle system is real future work (`architecture/performance.md` §18's own sketch), not this pass's job. |
| Real stacking-context nesting/paint-order edge cases (a layer's own descendant establishing an inner stacking context that should paint between two *different* z-index groups of the outer one) aren't modeled | High (accepted) | Documented scope cut: nested stacking contexts stay flattened into their nearest ancestor layer's own canvas this pass — real, but narrower, same as every other item this session scoped down. |

## Acceptance
- [ ] Task 1-4 complete, each independently testable
- [ ] `compositing_layer_test.rs`'s in-layer-vs-out-of-layer invalidation tests prove real per-layer caching (the actual point of item 28), not just correct compositing
- [ ] Full workspace build + test green
- [ ] `spec/ROADMAP.md` item 28 updated with the real scope cut (no nested layers, no per-primitive damage, coarse viewport/stylesheet invalidation)
