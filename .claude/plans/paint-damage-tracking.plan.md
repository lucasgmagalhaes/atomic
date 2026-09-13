# Plan: Paint damage tracking (whole-frame cache-and-skip)

**Complexity**: Small

## Requirements restatement

`spec/ROADMAP.md` item 13 (P1): *"Paint damage tracking (currently whole-frame repaint every render)."* Every `Page::render` call (`crates/profile/src/bin/profile_worker/page/render.rs`) re-walks the whole `LayoutBox` tree (`build_display_list`/`build_glyph_list`/`build_image_list`), re-submits a full GPU rect pass (`GpuRenderer::render_to_rgba`), and re-blits every image/glyph (`composite_images`/`composite_glyphs`) into a freshly allocated `Vec<u8>` — every single tick of `profile-worker`'s ~60Hz vsync loop (`main.rs:322`), even when nothing paint-relevant changed since the previous frame (the common case for an idle-game-shaped page ticking a `setTimeout` every 50ms while nothing touches the DOM in between, or a page that's simply sitting idle with no timers at all).

Goal: skip the paint pipeline and reblit the previous frame's already-composited pixels when nothing that could change what's on screen has changed since the last `render()` call for this `Page`.

## Pattern grounding

| Category | Source | Pattern to mirror |
|---|---|---|
| Cache-and-skip on unchanged inputs | `crates/profile/src/bin/profile_worker/page/render.rs:30-89` (`Page::layout`'s `LayoutCache`) | Exact shape to reuse for paint: a `RefCell<Option<Cache>>` field on `Page`, a cache struct carrying the exact inputs the result was computed from, a borrow-and-compare-then-early-return at the top of the method, an unconditional overwrite at the bottom on a miss. |
| Cheap per-call versioning already available | `dom::Dom::layout_version()`/`style_version()` (`crates/dom/src/query.rs`), `Context::adopted_stylesheet_version()` | Already the *exact* three-field cache key `LayoutCache` uses — reused verbatim for the paint cache, see Risks for why `DirtyFlags::PAINT` is deliberately **not** used instead. |
| Tests | `crates/profile/tests/profile_test.rs` | Same real-process-spawn integration convention every other `profile-worker` behavior test in this file uses (spawn the binary, send stdin verbs, read stdout/frame). |

No existing pattern for: a cached pixel buffer being returned instead of recomputed — this plan introduces that one new shape, kept as a straight `Vec<u8>` clone on a cache hit rather than a `Cow`/`Rc` reshape of `render`'s return type, to avoid touching `render`'s 13 call sites across `commands/*.rs` and `main.rs`.

## Investigation finding that changes the original approach

The task brief (and `DirtyFlags::PAINT`'s own existence) suggested reading `dom.drain_dirty()` each `render()` call and skipping paint when `PAINT`/`LAYOUT` weren't set. **This is unsound as the sole cache key**: `dom::Dom::focus`/`hover::set_hovered` (`crates/dom/src/hover.rs:9`, and `Dom::focus`) deliberately do **not** call `mark_dirty` — they only bump `style_version()` (that's the exact, documented reason `LayoutCache` already carries `style_ver` as a *separate* key from `layout_ver`, see that field's own doc and the regression test it cites, `clicking_to_focus_an_input_updates_its_real_computed_focus_style`). A `:hover`/`:focus` style change is genuinely paint-relevant (background/color/border can all key off `:hover`/`:focus`) but would never set `DirtyFlags::PAINT`, so a `drain_dirty()`-based cache would silently freeze a stale frame across the very case `LayoutCache` was already fixed to handle correctly.

**Revised approach**: reuse `LayoutCache`'s own three-field key (`layout_ver`, `style_ver`, `adopted_version`) for the paint cache too, extended with `width`/`height`/`scroll_top` (paint-affecting inputs `LayoutCache` doesn't need to distinguish beyond its own `width`/`height` fields, since scrolling doesn't change layout but does change what's visible). This is provably correct by construction — it's the same key that already gates the layout tree these very pixels are computed from, so "layout cache would return the same tree" is by definition true whenever "paint cache can return the same pixels" is (same width/height) — and `DirtyFlags`/`drain_dirty` stay unused for this feature. `DirtyFlags::PAINT` itself remains a real, if not-yet-consumed, signal for a *future*, more precise per-region damage pass (see Scope cut below) — nothing here removes or repurposes it.

## Files to change

| File | Action | Why |
|---|---|---|
| `crates/profile/src/bin/profile_worker/page/mod.rs` | UPDATE | Add `PaintCache` struct (`width`, `height`, `scroll_top: f64`, `layout_ver`, `style_ver`, `adopted_version: u64`, `pixels: Vec<u8>`) and a `paint_cache: RefCell<Option<PaintCache>>` field on `Page`. |
| `crates/profile/src/bin/profile_worker/page/render.rs` | UPDATE | `Page::render`: compute the cache key up front, return a clone of the cached pixels on a hit, otherwise run the existing pipeline unchanged and store the result before returning. |
| `crates/profile/tests/profile_test.rs` | UPDATE | New test(s): (a) two consecutive renders of a static page (no mutation in between) produce byte-identical pixels — proves the cache path doesn't corrupt output; (b) a real DOM mutation between two renders (e.g. via `EVAL` changing `textContent`) produces a *different* frame — proves the cache doesn't go stale; (c) a `:hover`/`:focus`-only change (no `mark_dirty`) between two renders still produces a different frame — the regression this plan's own investigation exists to prevent. |
| `spec/ROADMAP.md` | UPDATE | Flip item 13 to `[~]` with the same "done, scoped" note style items 23–25 already use, naming the honest scope cut (see below). |

## Tasks

### Task 1: `PaintCache` struct + field
- **Action**: Add the struct and field to `Page` (`page/mod.rs`), doc-commented the same way `LayoutCache` is (what it caches, why each field is a required key).
- **Mirror**: `LayoutCache`'s existing doc-comment shape and field list.
- **Validate**: `cargo build -p profile`.

### Task 2: cache-and-skip in `Page::render`
- **Action**: At the top of `render`, fetch `dom.layout_version()`/`style_version()`/`self.ctx.adopted_stylesheet_version()` (same three calls `layout()` already makes internally — cheap `u64` reads, duplicating the three getter calls rather than threading them through is the right call per KISS: no new plumbing, no signature change to `layout()`), compare against `self.paint_cache`, return `cached.pixels.clone()` on a full match (`width`/`height`/`scroll_top`/all three versions). On a miss, run the existing body unchanged, then store the result in `self.paint_cache` before returning it.
- **Mirror**: `Page::layout`'s borrow-compare-return-or-rebuild shape exactly.
- **Validate**: `cargo build -p profile`.

### Task 3: tests proving correctness (not just that it compiles)
- **Action**: the three cases listed above in `profile_test.rs`.
- **Mirror**: existing spawn-and-drive-the-real-binary tests in the same file (e.g. whichever test already asserts on rendered pixel content today — reuse its exact spawn/stdin/frame-read helpers, don't invent new ones).
- **Validate**: `cargo test -p profile --release -- --test-threads=1` (single-threaded per this repo's own documented convention for real-process tests, see the P3 plan's Validation section).

### Task 4: document the scope cut
- **Action**: Update `spec/ROADMAP.md` item 13 to `[~]`, stating plainly: this is a *whole-frame* cache-and-skip (identical inputs ⇒ identical output, reblit instead of recompute), not per-region/per-`LayoutBox` dirty-rect tracking (`spec/architecture/performance.md` §18's `DamageRegion { rects: Vec<Rect> }` sketch) — a page where *anything* paint-relevant changed still repaints its *entire* frame, same as today, just no longer repaints frames where *nothing* changed. `DirtyFlags::PAINT` stays unconsumed, reserved for that future, more precise pass.
- **Mirror**: items 23–25's own "done, scoped, here's exactly what's cut" phrasing.
- **Validate**: read-through, no build step.

## Validation

```bash
cargo build --workspace
cargo test -p profile --release -- --test-threads=1
cargo fmt --all
```

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Cache key misses a paint-relevant input, producing a stale frame | Low | Key is provably a superset of `LayoutCache`'s own (proven-correct-by-existing-use) key, plus `scroll_top` — the only paint input that isn't also a layout input. New tests explicitly cover the hover/focus case that motivated this analysis. |
| `pixels.clone()` on a cache hit is still a real allocation + `memcpy` (not truly free) | Certain, by design | Documented as the accepted cost of not reshaping `render`'s `Vec<u8>`-by-value signature across 13 call sites — still strictly cheaper than the full tree-walk + GPU submit + CPU composite it replaces. Not treated as a correctness risk, only a "how much is saved" ceiling. |
| A caller relies on `render()` having some other side effect on every call regardless of caching (e.g. `set_layout_rects`/`set_computed_styles` for `getComputedStyle`) | Medium | Checked in Task 2: `render`'s existing `self.ctx.set_layout_rects(...)`/`set_computed_styles(...)` calls happen via `self.layout(...)`'s own tree, which itself only recomputes on `LayoutCache`'s miss — on a paint-cache hit, the layout cache is *also* necessarily a hit (same key superset), so those calls still run against the identical (already cached) tree and set the identical values. No behavior change needed there. |

## Acceptance

- [ ] `PaintCache` added, `Page::render` skips the paint pipeline on a full cache hit
- [ ] 3 new `profile_test.rs` cases pass (identical-frame reuse, DOM-mutation invalidates, hover/focus-only invalidates)
- [ ] `cargo build --workspace` and `cargo test -p profile --release -- --test-threads=1` green
- [ ] `cargo fmt --all` clean
- [ ] `spec/ROADMAP.md` item 13 flipped to `[~]` with the scope-cut note
- [ ] Pattern mirrored (`LayoutCache`'s exact shape), nothing reinvented; `DirtyFlags`/`drain_dirty` deliberately left unused, with the reasoning recorded above rather than silently discarded
