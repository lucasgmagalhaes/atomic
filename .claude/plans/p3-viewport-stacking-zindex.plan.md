# Plan: P3 rendering foundations — viewport, stacking contexts, z-index

**Complexity**: Medium

## Requirements restatement

Three P3 items from `spec/ROADMAP.md`, done in dependency order, each shippable/testable on its own:

1. **Real viewport** — height threaded into layout/media queries the way width already is, plus `window.innerWidth`/`innerHeight`, plus a real live-resize verb for `profile-worker` (currently width/height are fixed CLI args for the process's whole life — no resize protocol exists at all).
2. **Stacking contexts** — the concept that a positioned+z-indexed box's descendants paint as one atomic unit, not interleaved with siblings.
3. **z-index** — the property that lets a stacking context be reordered against its siblings (negative behind, positive in front).

Chrome UI (`apps/shell`'s in-process `ChromeEngine`) already re-does layout every frame against `ui.available_width()`, so it gets viewport-width-resize "for free" — this plan's resize work is scoped to real profile pages (`profile-worker`), the actual gap.

## Pattern grounding

| Category | Source | Pattern to mirror |
|---|---|---|
| Two-way host↔JS scalar | `crates/js-runtime/src/window.rs:35-47,109-154`, `lib.rs:490-508` | `HostState` field + `Context::set_x`/`x()` + JS getter via `define_getter`. Mirror exactly for `innerWidth`/`innerHeight` (read-only — no setter needed, unlike `scrollY`). |
| Stdin verb in profile-worker | `crates/profile/src/bin/profile_worker/main.rs` (`if line == "PING"` / `"RELOAD"` / prefix-stripped verbs ~294-545) | New `RESIZE <w> <h>` verb follows the same `if/else if line.starts_with(...)` chain, re-runs layout + `writer.publish(...)`, same shape `RELOAD` already has. |
| New `ComputedStyle` field + parser dispatch | `crates/layout-engine/src/style.rs:239-360` (e.g. `border_radius`, `opacity`) | `z_index: Option<i32>` (None = `auto`, the initial value), parsed in the same `match property_name` dispatch, integer-only (no `calc()`), same "documented narrower scope" convention. |
| Media feature enum + `matches()` | `crates/css/src/parser.rs:161-188` | Add `MinHeight`/`MaxHeight`/`Height` variants mirroring `MinWidth`/`MaxWidth`/`Width`; `MediaQuery::matches` gains a `viewport_height: f64` parameter. |
| Paint-order tree walk | `crates/render/src/display_list.rs:134-171` (`build_display_list`/`collect`) | Insert a per-level stable sort of `box_.children` into 3 paint buckets before recursing — no new tree structure, reuses the existing recursive walk. |
| Tests | `crates/layout-engine/tests/style_test.rs`, `crates/render/tests/display_list_test.rs`, `crates/css/tests/`, `crates/profile/tests/profile_test.rs` | Same integration-style `crate/tests/*_test.rs` convention (never inline `#[cfg(test)]`), per `CLAUDE.md`. |

No existing pattern for: a stacking-context data structure (none exists — this plan introduces the concept from scratch, kept as a paint-time sort, not a persistent tree, to stay minimal per KISS/YAGNI).

## Files to change

| File | Action | Why |
|---|---|---|
| `crates/js-runtime/src/host_state.rs` (or wherever `HostState` lives) | UPDATE | Add `viewport_width: f64`, `viewport_height: f64` fields. |
| `crates/js-runtime/src/lib.rs` | UPDATE | `Context::set_viewport_size(w, h)`, `viewport_width()`, `viewport_height()` — mirrors `set_scroll_y`/`scroll_y`. |
| `crates/js-runtime/src/window.rs` | UPDATE | `innerWidth`/`innerHeight` read-only getters, registered in the same `register()` this file already has. |
| `crates/css/src/parser.rs` | UPDATE | `MediaFeature::{MinHeight, MaxHeight, Height}`; `MediaQuery::matches(&self, viewport_width, viewport_height)`. |
| `crates/css/src/cascade.rs` | UPDATE | `matching_declarations` gains `viewport_height` param, threads to `media.matches(...)`. |
| `crates/layout-engine/src/tree.rs` | UPDATE | `build_box_tree_with_viewport` gains a `viewport_height: f64` param (or a new `build_box_tree_with_viewport_size(w, h)` if changing the existing signature breaks too many call sites — decide in-place per call-site count), threads to `matching_declarations`. `vh`/`vw` length units stay out of scope (not parsed today) — this task only affects media queries, not `height` resolution. |
| `crates/profile/src/bin/profile_worker/main.rs` | UPDATE | New `RESIZE <w> <h>` stdin verb: updates local `width`/`height` mutable bindings, calls `ctx.set_viewport_size(...)`, re-runs layout, republishes via `writer.publish`. Real `"resize"` window event dispatch, same convention `RELOAD` uses for its own lifecycle events. |
| `crates/ipc` (`FrameWriter`) | CHECK, maybe UPDATE | `FrameWriter::new(shmem_name, width, height)` fixes the shared-memory buffer size at creation — a live resize needs either a larger fixed max buffer reused across sizes, or a new writer. Inspect before committing to an approach; highest-risk unknown in the whole plan. |
| `crates/layout-engine/src/style.rs` | UPDATE | `z_index: Option<i32>` field on `ComputedStyle`, `None` in `initial()`, parse arm (integer only, no `calc()`/`auto` string maps to `None`). |
| `crates/render/src/display_list.rs` | UPDATE | `collect()`: before the `for child in &box_.children` loop, build a stably-sorted index list into 3 buckets (negative-z-index stacking-context children first, everything else in tree order, non-negative-z-index stacking-context children last), iterate in that order instead. A box "establishes a stacking context" this pass iff `position != Static && z_index.is_some()`. |
| `crates/layout-engine/tests/style_test.rs` | UPDATE | `z-index` parse cases: default `None`, integer sets it, `z-index: auto` clears a previously cascaded value. |
| `crates/css/tests/parser_test.rs` (or wherever media-feature tests live) | UPDATE | `min-height`/`max-height`/`height` media-feature parse + `matches()` cases. |
| `crates/render/tests/display_list_test.rs` | UPDATE | Paint-order cases: negative z-index paints behind a non-positioned sibling; positive z-index paints in front; two positioned boxes with equal z-index preserve tree order (stable sort). |
| `crates/profile/tests/profile_test.rs` | UPDATE | New `resize_republishes_a_frame_at_the_new_dimensions` (or similar) end-to-end test, same real-process-spawn convention every other test in this file already uses. |
| `crates/js-runtime/tests/*` | UPDATE | `innerWidth`/`innerHeight` reflect a real `set_viewport_size` call. |

## Tasks, in dependency order

### Task 1: `window.innerWidth`/`innerHeight` (read-only host↔JS plumbing)
- **Action**: Add `viewport_width`/`viewport_height` to `HostState`, `Context::set_viewport_size`/`viewport_width()`/`viewport_height()`, JS getters.
- **Mirror**: `scrollY`'s exact shape, minus the setter half (`innerWidth`/`innerHeight` are spec-read-only).
- **Validate**: `cargo test -p js-runtime`.

### Task 2: media queries gain height
- **Action**: `MediaFeature` height variants, `MediaQuery::matches` takes `viewport_height`, thread through `cascade.rs`/`tree.rs`.
- **Mirror**: existing width variants exactly.
- **Validate**: `cargo test -p css -p layout-engine`.

### Task 3: real live resize (`RESIZE` verb)
- **Action**: new stdin verb in `profile_worker::main`, mutable `width`/`height`, `set_viewport_size`, re-layout, republish, real `"resize"` event.
- **Mirror**: `RELOAD`'s handling shape.
- **Validate**: `cargo test -p profile` — new end-to-end test spawning a real process, sending `RESIZE`, asserting the republished frame's dimensions and a `getComputedStyle`-visible media-query flip.
- **Blocking unknown**: confirm `ipc::FrameWriter`/shared-memory sizing tolerates a size change (see Risks) before finishing this task — may need a small `ipc` change first.

### Task 4: `z-index` property
- **Action**: `ComputedStyle::z_index: Option<i32>`, initial `None`, parser arm.
- **Mirror**: `border_radius`/`opacity`'s existing numeric-parse-and-store shape.
- **Validate**: `cargo test -p layout-engine` (new `style_test.rs` cases).

### Task 5: stacking-context paint-order sort
- **Action**: bucket-sort in `display_list::collect` as scoped above.
- **Mirror**: `collect`'s existing recursive-walk shape — no new tree type.
- **Validate**: `cargo test -p render` (new `display_list_test.rs` cases) + full existing suite (nothing regresses — z-index absent/`auto` on every current box must paint byte-identical to today).

### Task 6 (fast follow, same commit or next): document the scope cut
- Real spec detail intentionally **not** modeled this pass (state explicitly in doc comments, matching this codebase's "honest gap" convention): `position != Static` with `z-index: auto` does **not** get its own separate later paint step ahead of non-positioned siblings (real CSS2.1 step 3 vs step 5 distinction) — this pass's bucket 2 merges them. `opacity < 1` and (future) `transform` don't trigger a stacking context either, matching the existing "opacity multiplies into paint alpha, no offscreen layer" scope cut already documented on `ComputedStyle::opacity`.

## Build order (small, per-domain commits, per CLAUDE.md convention)

1. `feat(js-runtime): real window.innerWidth/innerHeight`
2. `feat(css,layout-engine): min-height/max-height/height media features`
3. `feat(profile): live RESIZE — real viewport resize end to end` (bundles the ipc check/fix if one's needed)
4. `feat(layout-engine): parse and store z-index`
5. `feat(render): stacking-context paint order for position+z-index`

Each must pass `cargo build --workspace` before the next, per `spec/RULES.md`.

## Validation

```bash
cargo build --workspace
cargo test --workspace -- --test-threads=1
cargo fmt --all
```
(single-threaded per this session's earlier finding: parallel real-process tests in `profile`/`import` flake under contention, not a real regression signal.)

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| `ipc::FrameWriter`'s shared-memory region is fixed-size at creation, so `RESIZE` to a larger size doesn't fit | Medium | Inspect `crates/ipc/src/*.rs` first (Task 3); either allocate a generously-padded max buffer up front, or recreate the shared-memory segment on resize (same name, new size) and have the shell side re-map — decide before writing the verb handler. |
| Bucket-sort scope cut (Task 6) surprises a later feature that assumes full CSS2.1 paint-order fidelity | Low | Documented inline per this codebase's convention; revisit only if a real target-content page needs it (per `CLAUDE.md`'s "hybrid niche" scope decision — don't chase full fidelity speculatively). |
| Changing `build_box_tree_with_viewport`'s signature ripples across every call site (chrome bundles + profile-worker + tests) | Medium | Grep call sites before touching the signature; prefer adding a new `_with_viewport_size` function over breaking the existing one if call-site count is large (mirrors this repo's own "don't break existing callers" precedent from the `border-radius` `Rect` field addition). |
| `z-index` interacting with existing `opacity`-compounding paint pass in an unexpected order | Low | New display_test cases explicitly cover z-index + opacity together. |

## Acceptance

- [ ] All 5 tasks complete, each its own commit
- [ ] `cargo build --workspace` and `cargo test --workspace -- --test-threads=1` green
- [ ] `cargo fmt --all` clean
- [ ] `spec/matrix/css-layout.md` / `spec/matrix/paint.md` / `spec/ROADMAP.md` items 23 (Viewport), 24 (Stacking contexts), 25 (z-index) flipped `[ ]` → `[x]`/`[~]` with the scope-cut note from Task 6
- [ ] Patterns mirrored (host-state two-way binding, stdin verb, `ComputedStyle` field+parser, media feature), nothing reinvented
