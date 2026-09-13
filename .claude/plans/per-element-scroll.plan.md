# Plan: Real Per-Element Scroll — `scrollTop`/`scrollLeft`/`scroll()`/`scrollTo()`/`scrollBy()` (ROADMAP item 22)

**Source**: `spec/ROADMAP.md` item 22's own "Still `[ ]`" note — the document/viewport-level scroll (`window.scrollY`/`scrollTo`/the `SCROLL` command) already landed; this is the one remaining sub-scope: an arbitrary `overflow: auto`/`scroll` container's own scroll methods, currently no-op stubs in `crates/js-runtime/src/dom_bindings/scroll_focus.rs`.
**Complexity**: Large

## Summary

Give any element real, independent scroll state: `scrollTop`/`scrollLeft` read/write a per-`NodeId` offset stored on `dom::Dom` (same "own field, no layout/style dirty" shape `Dom::focus`/`hover` already use), clamped against the element's own real content extent (`scrollHeight`/`scrollWidth`, new — computed the same way `profile-worker`'s existing `collect_layout_rects` walk already flattens a `LayoutBox` tree). `render::display_list`'s three collectors (`build_display_list`/`build_image_list`/`build_glyph_list`) shift a scrolled container's *children* by its own offset — the exact same accumulated-translate-plus-clip mechanism `transform`/`overflow: hidden` already have, just subtracting the element's own scroll offset instead of adding a `transform`. `scroll()`/`scrollTo()`/`scrollBy()` become real wrappers around the same setter.

Per the plan's own grounding: `Overflow::Hidden` already collapses `hidden`/`auto`/`scroll` into one clipped variant (real CSS allows programmatic `scrollTop`/`scrollLeft` on all three — only `visible` isn't a scroll container), so no new `Overflow` variant is needed. `scrollIntoView()` stays a no-op — out of scope, not requested.

## Patterns to Mirror

| Category | Source | Pattern |
|---|---|---|
| Per-node real state, no dirty-flag misuse | `crates/dom/src/focus.rs`'s `Dom::focus`/`blur` | A dedicated field bumping only the dirty flags that are actually true (focus bumps `style_version`, never `layout_version`) — scroll needs the same care: it changes paint, never layout or style/selector matching. |
| Accumulated per-box translate + clip during paint | `crates/render/src/display_list/rects.rs`'s `collect` (the `transform`/`translate` accumulation, and `Overflow::Hidden`'s `tighten_clip`) | A per-element scroll shift is the same shape as the existing translate accumulation, except it applies only to *children*, not the box's own background/border/shadow, and it's *subtracted* rather than added (matches the whole-document `scroll_top` shift's own `y - offset` sign convention in `profile-worker`'s `Page::render`). |
| Flattening a `LayoutBox` tree into a `NodeId`-keyed snapshot for a host-state-backed JS API | `crates/profile/src/bin/profile_worker/layout_snapshot.rs`'s `collect_layout_rects`/`collect_computed_styles` | The new `scrollHeight`/`scrollWidth` need the exact same "one more `collect_*` walk, pushed via a new `Context::set_*` call, read back by a `js_runtime` getter through `HostState`" pipeline `getBoundingClientRect`/`getComputedStyle` already use. |
| Host-state-backed getter | `crates/js-runtime/src/layout_measurement.rs`'s `rect_for`/`client_width_get` | Same `node_id` → `host_state::get` → `HashMap` lookup → default-on-miss shape. |
| Tests | `crates/render/tests/overflow_test.rs` (clip), `crates/js-runtime/tests/element_scroll_test.rs` (current stub tests), `crates/profile/tests/scroll_behavior_test.rs` (whole-page scroll's own pixel-level proof) | Mirror all three: a layout-engine/render-level clip+shift test, a `js-runtime`-level getter/setter/clamping test, and a `profile`-level real pixel-shift test. |

## Files to Change

| File | Action | Why |
|---|---|---|
| `crates/dom/src/lib.rs` | UPDATE | Add `element_scroll: HashMap<NodeId, (f64, f64)>` field to `Dom`. |
| `crates/dom/src/scroll.rs` | CREATE | `Dom::element_scroll_offset(id) -> (f64, f64)` (default `(0.0, 0.0)`) and `Dom::set_element_scroll_offset(id, x, y)` — bumps `DirtyFlags::PAINT` only (real repaint on scroll, no layout/style recompute), no style invalidation (scroll state isn't selector-matchable in this engine's scope, same as today). |
| `crates/layout-engine/src/tree/types.rs` | UPDATE | `LayoutBox` gains `pub scroll_offset: (f64, f64)`. |
| `crates/layout-engine/src/tree/build.rs` (or wherever a box's `style`/`dimensions` are populated from `dom`) | UPDATE | Populate `scroll_offset` from `dom.element_scroll_offset(node)` at box-tree construction time — same "read once from `dom` while building the tree" convention every other per-box field already follows. |
| `crates/render/src/display_list/rects.rs`, `images.rs`, `glyphs.rs` | UPDATE | Each `collect`/`collect_images`/`collect_glyphs` subtracts `box_.scroll_offset` from the translate passed to *children only* (box_'s own background/border/shadow paint at the un-scrolled translate) when `box_.style.overflow == Overflow::Hidden`. |
| `crates/profile/src/bin/profile_worker/layout_snapshot.rs` | UPDATE | New `collect_scroll_extents(tree) -> HashMap<NodeId, (f64, f64)>` — each box's own children's bounding extent (max `child.y + child.height`/`child.x + child.width` relative to the box's own origin), the real `scrollHeight`/`scrollWidth` source data. |
| `crates/js-runtime/src/host_state.rs` | UPDATE | `HostState` gains `scroll_extents: HashMap<dom::NodeId, (f64, f64)>`. |
| `crates/js-runtime/src/context/mod.rs` | UPDATE | `Context::set_scroll_extents(&mut self, extents: HashMap<dom::NodeId, (f64,f64)>)`, mirroring `set_layout_rects`/`set_computed_styles`. |
| `crates/js-runtime/src/layout_measurement.rs` | UPDATE | Add real `scrollHeight`/`scrollWidth` getters — `max(scroll_extents value, own clientHeight/clientWidth)` (a container's scrollable extent is never smaller than its own visible box). |
| `crates/js-runtime/src/dom_bindings/scroll_focus.rs` | UPDATE | Real `scrollTop`/`scrollLeft` getter (reads `dom.element_scroll_offset`) and setter (clamps to `[0, scrollHeight - clientHeight]`/`[0, scrollWidth - clientWidth]`, floors negative ranges to `0`, calls `dom.set_element_scroll_offset`). `scroll()`/`scrollTo()`/`scrollBy()` become real: parse either `(x, y)` numeric args or a `{top, left}` options object (matching the existing `window.scrollTo`'s own argument-parsing shape, if any — otherwise a small dedicated parser), and set (`scrollTo`) or add-to (`scrollBy`) the current offset via the same setter. `scrollIntoView` untouched. |
| `crates/profile/src/bin/profile_worker/page/render.rs` | UPDATE | Call `ctx.set_scroll_extents(collect_scroll_extents(&tree))` alongside the existing `set_layout_rects`/`set_computed_styles` calls. |
| `crates/dom/tests/scroll_test.rs` | CREATE | `element_scroll_offset` default/set/read round-trip; confirms `layout_version`/`style_version` are untouched by a scroll-offset change (only `PAINT` dirty flag). |
| `crates/render/tests/element_scroll_test.rs` | CREATE | A box with `overflow: hidden`/a nonzero `scroll_offset` shifts its children's painted rects by that offset while its own background/border stay fixed, still clipped to its own border box. |
| `crates/js-runtime/tests/element_scroll_test.rs` | UPDATE | Replace the current stub-behavior assertions (`scrollTop` always `0`, setters no-op) with real ones: set/get round-trip, clamping against a real `scroll_extents`/`layout_rects` fixture, `scrollTo`/`scrollBy` both forms (numeric args and options object). |
| `crates/profile/tests/scroll_behavior_test.rs` | UPDATE | New test: a real `overflow: auto` container's `scrollTop` set via page script visibly shifts only its own content in the rendered frame, leaving content outside it untouched — same pixel-level proof style the existing whole-page scroll tests already use. |
| `spec/ROADMAP.md` | UPDATE | Flip item 22's "Still `[ ]`" note once landed (or the whole item, if this closes it fully net of the already-accepted horizontal-scroll scope cut). |

## Tasks

### Task 1: Real per-element scroll storage in `dom::Dom`
- **Action**: `element_scroll: HashMap<NodeId, (f64, f64)>` field; `element_scroll_offset`/`set_element_scroll_offset` in a new `scroll.rs`, mirroring `focus.rs`'s file-per-concern split. Only `DirtyFlags::PAINT` bumped.
- **Mirror**: `focus.rs`/`hover.rs`'s own "dedicated field, precise dirty flags" shape.
- **Validate**: `cargo test -p dom --release -- --test-threads=1` (new `scroll_test.rs`).

### Task 2: `LayoutBox::scroll_offset` populated at box-tree build time
- **Action**: New field, set from `dom.element_scroll_offset(node)` in the same pass that already resolves each box's `ComputedStyle`/`dimensions`.
- **Mirror**: However `style`/other per-box fields are already threaded through `tree/build.rs`'s recursive builder.
- **Validate**: `cargo test -p layout-engine --release -- --test-threads=1` (no behavior change yet, just confirms the field populates and defaults to `(0,0)` for every existing test).

### Task 3: Real per-element scroll shift in `render::display_list`
- **Action**: In each of `rects.rs`/`images.rs`/`glyphs.rs`'s `collect`, when recursing into `box_.children`, pass `child_translate = translate - box_.scroll_offset` (only when `box_.style.overflow == Overflow::Hidden`) instead of the plain `translate` used today — `box_`'s own background/border/shadow still paint at the un-shifted `translate`. The existing `child_clip` (`tighten_clip`) already bounds this correctly with no changes.
- **Mirror**: The existing `transform`/`opacity` accumulation shape in the same functions.
- **Validate**: `cargo test -p render --release -- --test-threads=1` (new `element_scroll_test.rs`: a scrolled child's rect moves; its parent's own background doesn't; content scrolled out of the clip region is dropped, same shape `overflow_test.rs`'s existing clip tests already use).

### Task 4: `scrollHeight`/`scrollWidth` real backing data
- **Action**: `collect_scroll_extents` in `layout_snapshot.rs` (profile-worker); `HostState.scroll_extents` + `Context::set_scroll_extents` (js-runtime); `scrollHeight`/`scrollWidth` getters in `layout_measurement.rs`.
- **Mirror**: `collect_layout_rects`/`set_layout_rects`/`client_width_get`'s existing three-hop pipeline exactly.
- **Validate**: `cargo build -p profile -p js-runtime` (proven end to end in Task 5/6).

### Task 5: Real `scrollTop`/`scrollLeft`/`scroll()`/`scrollTo()`/`scrollBy()`
- **Action**: Getter reads `dom.element_scroll_offset`; setter clamps against `scrollHeight - clientHeight`/`scrollWidth - clientWidth` (floor `0` when content doesn't overflow) before calling `dom.set_element_scroll_offset`. `scroll()`/`scrollTo()` set absolute; `scrollBy()` adds to the current offset before the same clamp. Both call-shapes (`fn(x, y)` and `fn({top, left})`) parsed, matching real spec overloads.
- **Mirror**: `window.scrollTo`'s own existing argument handling, if it already parses both forms (reuse rather than reinvent); `scroll_offset_get`/`_set`'s existing getter/setter registration shape in `scroll_focus.rs`.
- **Validate**: `cargo test -p js-runtime --release --test element_scroll_test -- --test-threads=1`.

### Task 6: Wire the host + page-level test
- **Action**: `Page::render` calls `ctx.set_scroll_extents(...)` alongside its existing `set_layout_rects`/`set_computed_styles` calls. New `profile`-level pixel test: an `overflow: auto` container taller-than-its-box, scrolled via `el.scrollTop = N` from a page script, repaints with only that container's own content shifted.
- **Mirror**: `scroll_behavior_test.rs`'s existing whole-page scroll pixel-level assertions.
- **Validate**: `cargo test -p profile --release --test scroll_behavior_test -- --test-threads=1`.

## Validation

```bash
cargo test -p dom -p layout-engine -p render -p js-runtime -p profile --release -- --test-threads=1
cargo build --workspace --exclude shell
cargo test --workspace --exclude shell --exclude automation --release -- --test-threads=1
cargo fmt --all
```

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Clamping against a stale `scroll_extents`/`layout_rects` snapshot (pushed only once per `render()` call) could let a script read/write a scroll position based on outdated content size right after a DOM mutation that hasn't re-rendered yet | Medium | Same staleness window `getBoundingClientRect`/`getComputedStyle` already accept (both are "point-in-time snapshot, refreshed each `render()`" by design, documented in their own modules) — consistent, not a new gap. |
| Scroll shift interacting with `transform`/nested `overflow: hidden` ordering could accumulate incorrectly (wrong sign, or applied to the wrong box) since three different offsets (`transform`, scroll, already-existing translate accumulation) now compose in the same function | Medium | Task 3's own tests must cover a scrolled *and* transformed ancestor together, not just scroll alone, to catch a sign/ordering bug before it ships. |
| `scrollWidth`/horizontal scroll: `layout-engine` has no real horizontal-overflow model (documented existing scope cut) | High (accepted) | `scrollLeft`/`scrollWidth` still get real storage/getters/setters for API completeness (matching `window.scrollX`'s own "always 0, matches no-horizontal-overflow scope" existing precedent), but in practice `scrollWidth` will rarely exceed `clientWidth` since nothing in this engine produces horizontal overflow — a real, honestly-scoped no-op in practice rather than a fake success. |
| Three near-identical `collect`/`collect_images`/`collect_glyphs` functions all need the same fix — a missed one silently breaks image/glyph scrolling while rect scrolling works | Medium | Task 3's test suite explicitly exercises a scrolled container with a background rect *and* an image *and* text, not just one primitive kind. |

## Acceptance
- [ ] Task 1-6 complete, each independently testable
- [ ] A real `overflow: auto`/`scroll` element's `scrollTop`/`scrollLeft` read/write real, clamped state
- [ ] `scroll()`/`scrollTo()`/`scrollBy()` are real, not no-ops
- [ ] A scrolled container's content visibly shifts in rendered pixels (rects, images, and text all three), clipped to its own border box, without moving anything outside it
- [ ] Full workspace build + test green
- [ ] `spec/ROADMAP.md` item 22 updated with the real scope cut (no scrollbar drawn, no `behavior: smooth`, `scrollWidth`/horizontal scroll real but moot given the engine's no-horizontal-overflow scope, `scrollIntoView` still a no-op)
