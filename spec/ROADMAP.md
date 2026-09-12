# Roadmap — Actionable Queue

**Start here to pick the next task.** This is the P0→P5 priority order from `architecture/priority-roadmap.md`, reconciled against actual current status (matrix files + `CLAUDE.md` + git history) as of 2026-08-26 — the original priority list was aspirational and did not track what had already landed. Status markers: `[x]` done, `[~]` partial (see note), `[ ]` not started.

**Before picking up any `[ ]`/`[~]` item: read [RULES.md](RULES.md) first.** It's short. Each item below links to its architecture write-up (the *how*) and matrix section (the *current detail*).

## Product scope decision (2026-08-26)

Discussed and settled: this engine targets a **hybrid niche**, not general-purpose web browsing and not purely a learning exercise either. Concretely:

- **Target content**: controlled/curated sites this product is built for (idle games, dashboards, kiosk-style content) — not "any site on the internet." Full WPT/general-web conformance is explicitly *not* the near-term goal; `architecture/compatibility.md`'s WPT subset targets should stay scoped to what the actual target content needs, not chased for its own sake.
- **Where this engine can genuinely compete on performance**: memory footprint per idle tab/profile, cold-start time, idle CPU — real, measurable wins from QuickJS-ng (no JIT/huge heap) plus a minimal per-profile process versus Chromium's much heavier baseline.
- **Where it cannot compete**: raw JS execution throughput (V8's JIT vs QuickJS's interpreter), complex layout/paint throughput (Skia + GPU compositor vs this engine's current whole-frame repaint — see P1/P3's still-`[ ]` items: paint damage tracking, compositing layers, incremental layout). Don't market or optimize toward beating Chromium on these axes — it's not realistic for a small engine.
- **Implication for prioritization**: features that unblock "a well-behaved modern site renders correctly" (ES Modules, real scrolling/viewport, stacking contexts/z-index — P2/P3) matter more than exotic web-platform completeness (Shadow DOM, Custom Elements, CSS Grid — P5) *unless* the target content actually needs them.
- **No fingerprint spoofing, decided (2026-08-26)** — stays as `mockup/browser-idle-spec.md`'s Requisitos already state ("sem fingerprint spoofing"). This was re-confirmed after weighing it against the antidetect/multi-account-browser market (GoLogin/Multilogin/AdsPower) — that market's dominant real-world use is evading platforms' multi-account bans, which creates real business risk this project isn't taking on (payment-processor/app-store policy exposure, unclear legal liability for the tool vendor, no control over how it's marketed once built). **Practical effect**: the audience for the multi-profile side of this product is *legitimate* multi-account use — teams managing several real accounts openly, QA/testing multiple sessions, idle-game account farming on platforms that don't aggressively fingerprint-ban — not the ban-evasion market. Session/cookie isolation per profile (already the architecture) is the actual product; do not add canvas/WebGL/UA fingerprint randomization as a "competitive feature" without this decision being revisited explicitly with the user first.

---

## P0 — Architectural Foundations

Read [architecture/primitives.md](architecture/primitives.md) for all of these.

1. `[x]` **Non-destructive detached DOM nodes** — done (2026-08-26): `removeChild`/`remove`/`replaceChild`/`replaceWith` and the `innerHTML`/`outerHTML` setters now unlink via `Dom::remove_from_parent` instead of freeing the arena slot — a removed node keeps its identity/listeners/subtree and stays reattachable. See [matrix/dom.md](matrix/dom.md). `Destroyed` (generation mismatch) is now unreachable from any JS-facing API; only `Connected`/`Detached` occur in practice.
2. `[x]` **Generation-safe Node handles** — already done: `dom::Dom` uses generation-tagged `NodeId`s (see `CLAUDE.md` phase 1, tested in `crates/dom/tests/dom_test.rs`).
3. `[x]` **Central Mutation Pipeline** — done (2026-08-26): every DOM mutation goes through `Dom::mark_dirty(flags)` which classifies the mutation into `DirtyFlags` (DOM/COLLECTION/SELECTORS/STYLE/LAYOUT/PAINT/A11Y) and bumps a backward-compatible `mutation_count()`. A separate `layout_version()` counter increments only when the LAYOUT flag is set, used by `profile-worker`'s layout cache. Consumers drain flags via `drain_dirty()` (resets) or peek via `dirty_flags()`/`layout_version()`. See [matrix/dom.md](matrix/dom.md).
4. `[x]` **Dirty Flags** — done (2026-08-26): `DirtyFlags` bitflags in `crates/dom/src/lib.rs` classifies mutations into 7 subsystem categories. Structural mutations (append/insert/remove/replace) set all flags. Attribute mutations set SELECTORS+STYLE+LAYOUT+PAINT (conservative — DOM layer can't know which attributes affect layout). Text mutations set DOM+LAYOUT+PAINT. `set_value` sets DOM only. `focus`/`blur` set nothing. `drain_dirty()` resets flags after reading; `dirty_flags()` peeks without resetting.
5. `[~]` **Task Scheduler** — partial: `Context::run_pending_timers()` (`timers::pump` + `fetch_async::pump` + `JS_ExecutePendingJob` loop) is a real but manual/cooperative pump, not a prioritized multi-queue scheduler. See [matrix/runtime.md](matrix/runtime.md).
6. `[~]` **Resource Loader** (one shared URL→security→CSP→mixed-content→cache→network→decode pipeline). partial (2026-08-26): the "Cache" step now exists — `profile-worker`'s `network::ResourceCache` dedups repeat GETs (same URL referenced by more than one `<link>`/`@import`/`<img>`) within a single page load, sitting in front of the "Network" step (`fetch_with_cookies`, already shared by every resource kind). Still not unified: URL resolution (`page_source::resolve_url`) and the fetch call are still separate functions threaded through by hand rather than one `ResourceRequest`-shaped abstraction; CSP/mixed-content enforcement is still deliberately scoped to script-initiated `fetch`/XHR only (see `js-runtime/src/csp.rs`, `cors.rs`'s own docs) and not extended to `<link>`/`<img>` — a documented scope cut, not something this pass changed. See [matrix/changelog.md](matrix/changelog.md).
7. `[x]` **Structured Clone** — done (2026-08-26): `js_runtime::value_bridge::deep_clone` (a real `JSValue` → `storage::value::Value` → `JSValue` round trip through an intermediate that owns no `JSValue`s — no `JS_DupValue` reference sharing) is exposed to script as the global `structuredClone(value)` and reused natively by `history.pushState`/`replaceState` for `state` (closing that module's own previously-documented "kept by reference" deviation). Same documented scope as the `indexedDB` bridge it reuses: no `Date`/`Map`/`Set`/typed arrays/`RegExp`, no cycle detection. `workers`' `postMessage` still hand-escapes strings rather than using this — that rework is P4 item 33, which explicitly waits for MessagePort (items 29–30) first rather than bolting structured clone onto the existing ad hoc API. See [matrix/changelog.md](matrix/changelog.md).

## P1 — Performance Infrastructure

Read [architecture/performance.md](architecture/performance.md).

8. `[ ]` String interning / Atoms for tags, attributes, CSS properties.
9. `[ ]` Selector matching cache.
10. `[~]` Computed style cache — `getComputedStyle` exists (point-in-time snapshot, correct but not cached/incremental); the layout cache (P0 item 3) covers layout, not style specifically.
11. `[ ]` Dirty style propagation (inherited vs non-inherited property changes).
12. `[~]` DOM mutation batching — achieved as a side effect of `profile-worker`'s once-per-frame render loop (layout only runs when `render()`/`hit_test_at()` is called, not per mutation), not via an explicit `begin/end_mutation_batch` API. Fine for now; revisit if a caller ever needs layout mid-script-turn.
13. `[ ]` Paint damage tracking (currently whole-frame repaint every render).
14. `[x]` JS↔Rust boundary benchmarks — done (2026-09-12): `crates/dom/benches/dom_bench.rs`, `crates/css/benches/css_bench.rs`, `crates/js-runtime/benches/native_bench.rs` cover [performance.md §21](architecture/performance.md)'s DOM/CSS/JS-native suites (`querySelector`/`innerHTML` excluded — not modeled at the `dom` crate's own level).

## P2 — High-Impact Compatibility

Read [architecture/compatibility.md](architecture/compatibility.md) and [matrix/runtime.md](matrix/runtime.md) / [matrix/browser-apis.md](matrix/browser-apis.md).

15. `[x]` **URL / URLSearchParams** — done (`crates/js-runtime`, git commits `7057013`/`0217d48`, 23 tests).
16. `[x]` **Fetch Core** (`Request`/`Response`/`Headers`/body consumption) — done this session (`crates/js-runtime/src/request_response.rs`); real cancellation is not (see item 17).
17. `[~]` **AbortController** / `AbortSignal` — done (2026-08-26) for the case `ROADMAP.md` itself called out: `crates/js-runtime/src/abort_controller.rs` gives real `AbortController`/`AbortSignal` classes (`.signal`, `.abort(reason)`, `.aborted`, `.reason`, `.throwIfAborted()`, `AbortSignal.abort(reason)`, real `"abort"` event dispatch), and `addEventListener`'s `signal` option is no longer a no-op — an already-aborted signal skips registration entirely, and a later `abort()` really removes the listener. Not done: `AbortSignal.timeout()`/`.any()`, `.onabort` IDL property, and `fetch`/`XMLHttpRequest` don't read a `signal` option yet (in-flight requests run on a real OS thread with no cancellation channel — a separate, larger follow-up). See [matrix/changelog.md](matrix/changelog.md).
18. `[~]` Script loading modes — the real prerequisite landed (2026-09-12): `<script src="...">` external scripts are now actually fetched and run (`profile::bin::profile_worker::page_source::load_scripts`, same `ResourceCache`/`resolve_url` pipeline `<link>`/`<img>` use), where previously only inline `<script>` text ever executed. Still `[ ]`: every script (inline or external) still runs synchronously/parser-blocking in document order — no real `defer`/`async` timing distinction, and `type="module"` is treated as a plain classic script (no ES module resolution, see item 19).
19. `[ ]` ES Modules (resolver, import maps, dynamic `import()`, module cache).
20. `[~]` Default actions — button/input submit/reset done (2026-09-12): an unprevented click on `<button>` (default type, or explicit `submit`) or `<input type="submit">` triggers its nearest ancestor `<form>`'s real `requestSubmit()`; `type="reset"` triggers `reset()` (`js_runtime::dom_bindings::forms::run_default_click_action`, hooked into both real tree-walking dispatch paths, `events::dispatch`/`dispatch_existing`). `<a href>` click navigation also done (2026-09-12): the request crosses the engine/host boundary via `HostState::pending_navigation`/`Context::take_pending_navigation`, and `profile-worker`'s `navigate_if_requested` performs a real navigation (same `fire_before_unload` gate + `load_source` `NAVIGATE` uses) after `CLICK`/`CLICK_AT`/`EVAL`. Still `[ ]`: focus navigation (Tab order already real per `dom::Dom::tab_order`, just not wired as a click default), text selection, per-element scrolling defaults.
21. `[~]` Form semantics — validity/constraint API, `labels`, `FormData` done; real `reset()`/`requestSubmit()` done (2026-08-26: cancelable `"reset"`/`"submit"` events, `reset()` restores `input`/`textarea` `.value` from the new `defaultValue`, `requestSubmit()` gates on real per-control validity); checkbox/radio `.checked` reset done (2026-09-12: real independent `.checked`/`.defaultChecked`, mirroring `.value`/`.defaultValue`'s exact architecture — see [matrix/dom.md](matrix/dom.md)). Still `[ ]`: `<select>` reset (needs per-`<option>` `defaultSelected` storage, a separate/larger piece since it's per-option, not per-control) and selection APIs (`setSelectionRange`).

## P3 — Rendering

Read [matrix/css-layout.md](matrix/css-layout.md) and [matrix/paint.md](matrix/paint.md).

22. `[~]` Real scrolling — done (2026-08-26) for the document/viewport level: `window.scrollY`/`pageYOffset`/`scroll`/`scrollTo`/`scrollBy` (`js_runtime::window`) now read/write the same real scroll offset `profile-worker`'s `SCROLL` command already painted with, two-way (`Context::scroll_y`/`set_scroll_y`, `sync_scroll` in `profile_worker::main` reconciles them before every paint, host-clamped against real content height either way) — proven end to end (`crates/profile/tests/profile_test.rs`'s `window_scroll_to_from_page_script_moves_the_real_painted_viewport`: a page script's own `window.scrollTo(...)` moves the actual painted pixels, not just a JS-facing number). Still `[ ]`: per-element `scrollTop`/`scrollLeft`/`scroll()` on an arbitrary `overflow: auto/scroll` container (`Element.prototype`'s own scroll methods, `dom_bindings/scroll_focus.rs`, stay no-op stubs — only the one document-level viewport is real), and horizontal scroll (`scrollX`/`pageXOffset` always `0`, matching `layout-engine`'s own no-horizontal-overflow scope).
23. `[~]` Viewport — done (2026-09-12): real `window.innerWidth`/`innerHeight` (`js_runtime::window`, backed by `Context::set_viewport_size`), `min-height`/`max-height`/`height` media features (`css::MediaFeature`, threaded through `matching_declarations`/`build_box_tree_with_viewport`), and a live `RESIZE <w> <h>` stdin verb on `profile-worker` that republishes under a new shared-memory segment plus fires a real `"resize"` event (`Context::fire_resize`) — see `crates/profile/tests/profile_test.rs`'s `resize_republishes_a_frame_at_the_new_dimensions_and_flips_a_height_media_query`. Chrome UI (`apps/shell`'s `ChromeEngine`) still uses `layout_engine::DEFAULT_VIEWPORT_HEIGHT` rather than a real per-surface height (no chrome bundle uses a height media query today, so this has no observable effect yet — see that call site's own comment).
24. `[~]` Stacking contexts — done (2026-09-12), scoped: a box establishes one iff `position != Static && z_index.is_some()` (an explicit integer, not `auto`) — matches real CSS's actual trigger, not `position` alone. See item 25 for the paint-order mechanism.
25. `[~]` z-index — done (2026-09-12): `layout_engine::ComputedStyle::z_index: Option<i32>` (parsed in `style.rs`, including the two-token `Delim('-')`+`Number` lookahead real negative values need — the lexer never folds a unary minus into one token) and `render::display_list`'s new `paint_order` bucket-sort (negative z-index stacking contexts paint first, everything else in tree order, non-negative z-index stacking contexts paint last — stable within each bucket). Documented scope cut: a `position != Static` box with `z-index: auto` doesn't get its own separate later paint step ahead of non-positioned siblings the way the full spec's step 3 vs step 5 does — this pass's middle bucket merges them. See `crates/render/tests/display_list_test.rs`'s z-index tests and `.claude/plans/p3-viewport-stacking-zindex.plan.md` for the full plan.
26. `[ ]` Transforms.
27. `[~]` Incremental layout — the whole-document mutation-count cache (P0 item 3) is the safe first step; true per-subtree incremental layout is still open.
28. `[ ]` Compositing layers.

## P4 — Parallelism

Read [architecture/primitives.md §12](architecture/primitives.md).

29. `[ ]` MessagePort.
30. `[ ]` MessageChannel.
31. `[ ]` Structured Clone integration (see P0 item 7).
32. `[ ]` Transferable objects.
33. `[~]` Worker — real OS-thread `Worker` already exists (`crates/workers`, genuine parallelism), but built on ad hoc string messages instead of MessagePort/StructuredClone; needs rework once items 29–31 land rather than bolting structured clone onto the existing API.

## P5 — Large Platform Features

Read [matrix/dom.md](matrix/dom.md).

34. `[ ]` CSS Grid.
35. `[ ]` `iframe`.
36. `[ ]` Multiple browsing contexts.
37. `[ ]` Cross-window messaging.
38. `[ ]` Shadow DOM.
39. `[ ]` Custom Elements.
40. `[ ]` MutationObserver.

Note: `DOMParser`/`XMLSerializer` (a *different*, already-real piece of the "templates, shadow DOM, custom elements" matrix bullet) are done — see [matrix/dom.md](matrix/dom.md). `<template>` element content is not.

---

## Items not on the P0–P5 list (still real gaps)

Pulled from matrix files, doesn't map cleanly onto the priority list above:

- **CSS**: full formatting contexts (table, multicolumn, ruby, list markers, fragmentation), fonts (`@font-face`, shaping, kerning, bidi, font loading API), animations/transitions/filters/gradients/masks/blend-modes/container-queries. See [matrix/css-layout.md](matrix/css-layout.md).
- **Paint**: Canvas API completion (paths, transforms, text metrics, `toBlob`/`toDataURL`, OffscreenCanvas), selection/caret, focus rings, text decorations, SVG, print/PDF. See [matrix/paint.md](matrix/paint.md).
- **DOM**: `<template>` content as a real isolated fragment; HTML parser insertion-mode fidelity for `document.write`. See [matrix/dom.md](matrix/dom.md).
- **Events**: pointer capture, hover/enter/leave, double-click, context menu, drag-and-drop, clipboard events, IME/composition; `SubmitEvent`/`TouchEvent`/`DragEvent` classes. See [matrix/events.md](matrix/events.md).
- **Standards**: Web Platform Test conformance target (see [architecture/compatibility.md §28](architecture/compatibility.md)) — start targeted subsets (DOM/Events/Selectors/URL/History/Timers/Fetch/Forms/Collections) rather than waiting for "platform readiness."
- **Accessibility tree** — not started anywhere.

---

[← back to spec/INDEX.md](INDEX.md)
