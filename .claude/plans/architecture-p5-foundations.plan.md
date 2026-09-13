# Plan: Architecture Restructuring for P1/P2/P4/P5 Roadmap Items

**Source**: `spec/ROADMAP.md` items 8, 11, 19, 28, 34-39 (no single PRD — user asked for an
architecture review, not a feature)
**Selected scope**: shared-infrastructure design only. No item below is implemented by this
plan; each task produces the crate/trait/module boundary a later, item-specific plan will
build on, per `spec/architecture/overview.md`'s "architecture before APIs" rule.
**Complexity**: Large (touches `dom`, `css`, `layout-engine`, `render`, `js-runtime`,
`profile-worker`)

## Summary

Ten remaining ROADMAP items split into two kinds:
- **Self-contained** (no shared infra missing): item 34 (CSS Grid) is a pure `layout-engine`
  algorithm addition — no restructuring needed, it's ready to plan/implement directly.
- **Blocked on a real architectural gap**: items 8, 11, 19, 28, 35-39 each need a piece of
  shared infrastructure that doesn't exist yet. Building each item's own version of that
  infrastructure ad hoc is exactly the "fragmentation" `spec/architecture/overview.md` §1
  warns against — five items (35-38) in particular all need the *same* missing piece: a
  first-class `Window`/browsing-context abstraction, because today `profile-worker`'s `Page`
  hardcodes one `Context` + one `dom::Dom` per OS process with no notion of "more than one
  document" at all.

This plan defines five foundation stages. Each stage is small enough to be its own
plan→implement→test→merge cycle (matching this session's established flow), ordered so each
later item's later plan finds the primitive already there instead of inventing one.

## Patterns to Mirror

| Category | Source | Pattern |
|---|---|---|
| New shared primitive as its own crate | `crates/dom`, `crates/css` (workspace crates, not modules-of-a-module) | A cross-cutting concern with no natural single owner crate gets its own crate, referenced by `Cargo.toml` `[dependencies]` the same way `js-runtime` depends on `dom`/`storage`/`css`. |
| Per-concern struct instead of one god-struct | `spec/architecture/primitives.md` §13, already the doc's own worked example (`PageState { navigation, security, lifecycle, layout, storage }`) | `HostState` (`crates/js-runtime/src/host_state.rs`) is already flat and growing — don't add fields for the new stages, compose a new substruct instead (see Task 3). |
| New JS class registration | `class_registry::ensure_class` / `ensure_*_class` idempotency pattern (`CLAUDE.md` Known Gotchas) | `Window`-per-context and Custom Elements' upgrade path both register classes — reuse the existing per-`Context` prototype-cache check, don't hand-roll a second registry. |
| Versioned lazy cache | `LayoutCache`/`PaintCache` (`crates/profile/src/bin/profile_worker/page/render.rs`, `page/mod.rs`) — key = `(width, height, layout_ver, style_ver, adopted_version)` | Dirty style propagation's per-subtree version counters (Task 2) and the module cache (Task 4) both follow this exact "cache keyed on a version tuple, recompute on mismatch" shape, not a new invalidation mechanism. |
| Plain-JS-object native binding (no class ID) | `abort_controller.rs`, `history.rs`, `mutation_observer.rs`, `message_channel.rs` | `Window` identity for cross-window messaging doesn't need a new quickjs class kind if it can be a plain object carrying a `__window_id`, mirroring `MessagePort`'s `__port_id`. |
| Tests | `crates/dom/tests/*_test.rs`, `crates/js-runtime/tests/*_test.rs` | Every stage below gets its own integration test file in the owning crate — no inline `#[cfg(test)]`. |

## Files to Change (by stage)

| File | Action | Why |
|---|---|---|
| `crates/atoms/` (new crate: `Cargo.toml`, `src/lib.rs`) | CREATE | Stage 1: interning primitive shared by `dom`/`css`. |
| `crates/dom/src/lib.rs`, `attributes.rs`, `mutation.rs` | UPDATE | Stage 1: `tag`/attribute names become `atoms::Atom`, not `String`. |
| `crates/css/src/**` (selector/property matching) | UPDATE | Stage 1: match against `Atom` instead of `&str`/`String` comparisons. |
| `crates/dom/src/lib.rs` (new `subtree_version` bookkeeping), `crates/dom/src/mutation.rs` | UPDATE | Stage 2: per-subtree style-invalidation scope (inherited vs non-inherited), extending `DirtyFlags`/`style_version` rather than replacing them. |
| `crates/layout-engine/src/style/mod.rs` | UPDATE | Stage 2: `resolve_style` consults subtree version to skip unaffected branches. |
| `crates/js-runtime/src/window_context.rs` (new) | CREATE | Stage 3: `BrowsingContext`/window-identity abstraction wrapping `Context` + `dom::Dom` + a `window_id`. |
| `crates/js-runtime/src/host_state.rs` | UPDATE | Stage 3: compose a `WindowState` substruct instead of adding more flat fields (per Task 3's Patterns row). |
| `crates/js-runtime/src/window_registry.rs` (new) | CREATE | Stage 3: process-wide (or `Runtime`-wide) registry of live `BrowsingContext`s, keyed by `window_id`, for `iframe.contentWindow`/`window.open`/cross-window `postMessage` lookups. |
| `crates/profile/src/bin/profile_worker/page/mod.rs` | UPDATE | Stage 3: `Page` gets a `window_id`; still one `Page` per OS process, but now nameable/addressable — the minimum needed before iframe/multi-context work can start without a rewrite. |
| `crates/js-runtime/src/module_loader.rs` (new) | CREATE | Stage 4: resolver + module cache for ES Modules, built on `fetch_async`'s existing pump/registry shape. |
| `crates/render/src/layer.rs` (new) | CREATE | Stage 5: `Layer` concept (id, stacking-context root, own pixel buffer) — groundwork for compositing, reusing this session's `PaintCache` key shape per layer instead of one whole-frame cache. |
| `spec/ROADMAP.md`, `spec/architecture/primitives.md` | UPDATE | Document each stage as landed, cross-reference which item(s) it unblocks. |

## Tasks

### Stage 1: String Interning / Atoms (unblocks item 8, speeds up items 9-11 and CSS matching generally)
- **Action**: New `atoms` crate: a global, `RwLock`-backed (not `thread_local!` — `workers` spawns real OS threads, each needing the same tag/attribute vocabulary) string interner producing `Copy + Eq + Hash` `Atom(u32)` handles, with a `well_known!` macro or static table for the ~50 common tag/attribute/CSS-property names so hot-path comparisons never touch the interner lock. `dom::NodeData::Element.tag` becomes `Atom`; `attributes: HashMap<String, String>` keeps `String` values (real content) but attribute *names* become `Atom`. `css` selector/property matching switches from `&str` equality to `Atom` equality.
- **Mirror**: `LayoutCache`'s "compute once, compare cheaply" shape — atoms make the *comparison* cheap the same way caches make *recomputation* cheap.
- **Validate**: `cargo test -p atoms -p dom -p css --release -- --test-threads=1`; a micro-benchmark under `crates/profile`'s existing bench convention showing selector-matching throughput improvement (item 14's JS↔Rust boundary benchmark precedent).

### Stage 2: Dirty Style Propagation (item 11)
- **Action**: Add a per-subtree version counter (`spec/architecture/primitives.md` §7.1's "future optimization" — do it now since it directly implements item 11, don't defer it further). A mutation's `push_mutation_record`/`mark_dirty` path already knows the target `NodeId`; walk up to compute which ancestor subtree root(s) need re-cascading, and classify the change itself as inherited-property-affecting vs non-inherited (a `class`/`style` attribute change is a superset; a `scrollTop`-only change is not, mirroring `spec/architecture/primitives.md` §3.2's own worked examples). `resolve_style` in `layout-engine` checks the subtree version before recomputing a branch, skipping unaffected siblings.
- **Mirror**: `LayoutCache`'s version-tuple-keyed skip logic (`page/render.rs`) and this session's `PaintCache` — same "cache key includes a version, compare before recomputing" shape, one level more granular (per-subtree instead of whole-document).
- **Validate**: `cargo test -p dom -p layout-engine --release -- --test-threads=1`; a regression test proving a mutation in one subtree does NOT bump another sibling subtree's version (the actual bug this item exists to prevent).

### Stage 3: Window / BrowsingContext Abstraction (unblocks items 35 iframe, 36 multiple browsing contexts, 37 cross-window messaging, and is a prerequisite most of 38 Shadow DOM's document-tree-within-a-tree model benefits from too)
- **Action**: This is the load-bearing stage — the one real gap, not a refactor of something that mostly works. Introduce `window_context::BrowsingContext`, wrapping today's `Context` + its `dom::Dom` + a new `window_id: WindowId` (a simple incrementing counter, process-wide via `AtomicU64`, no cross-process sharing needed since `profile-worker` is already one process per top-level tab). Add `window_registry`: a `thread_local!`/`RwLock` map from `WindowId` to whatever's needed to reach another context's inbox (reusing `message_channel`'s existing port-registry shape, not inventing a second messaging path) — this is what `window.open()`, `iframe.contentWindow`, and cross-window `postMessage` (item 37) will all resolve through once implemented. `profile-worker`'s `Page` gets a `window_id` field; still exactly one `Page` per OS process for now (no multi-Dom-per-process rewrite in this stage) — that's the honest scope cut, matching how items 13/40/29-31 were each scoped narrower than full spec this session. Compose `HostState` with a new `pub window: WindowState { id, parent: Option<WindowId>, kind: WindowKind }` substruct instead of bolting flat fields on, per the Patterns table.
- **Mirror**: `message_channel.rs`'s `thread_local! { static REGISTRIES: RefCell<HashMap<usize, ...>> }` keyed by `ctx as usize` — `window_registry` is the same shape keyed by `WindowId` instead.
- **Validate**: `cargo test -p js-runtime -p profile --release -- --test-threads=1`; a new test proving two `BrowsingContext`s can look each other up by `WindowId` and exchange a `MessagePort`-style message end-to-end (the actual mechanism item 37 needs, exercised here without yet building `iframe`/`window.open` on top of it).

### Stage 4: Resource Loader + Module Cache (unblocks item 19 ES Modules; also the shared primitive item 16 Fetch Core / item 18 script-loading-modes should already be converging on)
- **Action**: `module_loader.rs`: a resolver (bare specifier -> URL, relative resolution against the importing module's own URL) plus a `HashMap<Url, ModuleRecord>` cache, keyed and invalidated the same version-tuple way as `LayoutCache` (a module fetched once per navigation, never re-fetched mid-page — matches real ES module semantics, not an arbitrary simplification). Reuses `fetch_async`'s existing pump-based delivery (`Context::run_pending_timers` already sums every module's `pump`), not a new async mechanism.
- **Mirror**: `fetch_async.rs`'s registry-plus-pump shape exactly.
- **Validate**: `cargo test -p js-runtime --release -- --test-threads=1`.

### Stage 5: Compositing Layer Groundwork (unblocks item 28; item 24/25 stacking-context/z-index work, already P3, is this stage's real input)
- **Action**: `render::layer::Layer { id, stacking_context_root: NodeId, cache: PaintCache }` — one `PaintCache` (this session's item-13 struct) per stacking-context-establishing element instead of one whole-frame cache, so a mutation inside one layer doesn't force a full-frame reblit of layers that didn't change. This stage only introduces the `Layer` type and per-layer cache key; it does NOT implement real GPU layer compositing (texture atlasing, layer promotion heuristics) — that's item 28's own future plan, scoped narrower here on purpose.
- **Mirror**: This session's `PaintCache` (`page/mod.rs`/`page/render.rs`) — literally the same struct, instantiated per-layer instead of per-page.
- **Validate**: `cargo test -p render --release -- --test-threads=1`.

### Not restructured: CSS Grid (item 34)
No shared-infrastructure gap blocks it — `layout-engine`'s existing box-tree/style-resolution pipeline already has everything a new layout algorithm needs (same shape as Flexbox, already implemented). Skip straight to a normal item-specific `/plan` when picked up; do not fold it into this restructuring.

## Validation

```bash
cargo build --workspace --exclude shell
cargo test --workspace --exclude shell --exclude automation -- --test-threads=1
cargo fmt --all
```

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Atom migration (Stage 1) touches `dom`+`css` broadly, risking a large low-value diff | Medium | Land it as its own PR before any dependent stage; keep `HashMap<String,String>` values as-is (only names become atoms) to bound the diff. |
| Stage 3's `WindowId`/registry is speculative ahead of iframe actually being implemented (YAGNI tension) | Medium | Justified the same way Structured Clone was (per `spec/RULES.md`'s YAGNI note): three items (35, 36, 37) need the identical primitive, which is the documented DRY exception, not a default license to build ahead. Scope Stage 3 to registry + lookup + one round-trip test only — no `iframe`/`window.open` JS API in this stage. |
| Subtree versioning (Stage 2) introduces a second version-tracking axis alongside the existing global `layout_version`/`style_version`, risking drift between them | Medium | Subtree versions are additive/derived from the same mutation path that already bumps the global counters — never an independent source of truth; test that global and subtree versions never disagree on "did this change." |
| Five-stage plan is still large for one sitting | High (expected) | Each stage is its own plan→implement→test→commit→merge cycle, same cadence as this session's last three features — this document is the map, not one PR. |

## Acceptance
- [ ] Stage 1-5 each land as separate, independently mergeable PRs
- [ ] Each stage's own tests pass in isolation and alongside the full workspace suite
- [ ] `spec/architecture/primitives.md` and `spec/ROADMAP.md` updated per stage, cross-referencing which item(s) it unblocks
- [ ] No stage implements a full ROADMAP item itself — each stage's Definition of Done is "primitive exists and is tested," not "item N is done"
