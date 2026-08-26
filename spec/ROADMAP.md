# Roadmap — Actionable Queue

**Start here to pick the next task.** This is the P0→P5 priority order from `architecture/priority-roadmap.md`, reconciled against actual current status (matrix files + `CLAUDE.md` + git history) as of 2026-08-26 — the original priority list was aspirational and did not track what had already landed. Status markers: `[x]` done, `[~]` partial (see note), `[ ]` not started.

**Before picking up any `[ ]`/`[~]` item: read [RULES.md](RULES.md) first.** It's short. Each item below links to its architecture write-up (the *how*) and matrix section (the *current detail*).

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
14. `[ ]` JS↔Rust boundary benchmarks — no benchmark suite exists at all yet (see [performance.md §21](architecture/performance.md)).

## P2 — High-Impact Compatibility

Read [architecture/compatibility.md](architecture/compatibility.md) and [matrix/runtime.md](matrix/runtime.md) / [matrix/browser-apis.md](matrix/browser-apis.md).

15. `[x]` **URL / URLSearchParams** — done (`crates/js-runtime`, git commits `7057013`/`0217d48`, 23 tests).
16. `[x]` **Fetch Core** (`Request`/`Response`/`Headers`/body consumption) — done this session (`crates/js-runtime/src/request_response.rs`); real cancellation is not (see item 17).
17. `[ ]` **AbortController** / `AbortSignal` — not started. `addEventListener`'s `signal` option is a documented no-op today.
18. `[ ]` Script loading modes (parser-blocking/`defer`/`async`).
19. `[ ]` ES Modules (resolver, import maps, dynamic `import()`, module cache).
20. `[ ]` Default actions (links, buttons, forms, focus navigation, selection, scrolling, cancellation).
21. `[~]` Form semantics — validity/constraint API, `labels`, `FormData` done; real submission/reset semantics and selection APIs (`setSelectionRange`) are not (see [matrix/dom.md](matrix/dom.md)).

## P3 — Rendering

Read [matrix/css-layout.md](matrix/css-layout.md) and [matrix/paint.md](matrix/paint.md).

22. `[ ]` Real scrolling (scroll position/APIs are documented no-op stubs today — no real viewport exists).
23. `[ ]` Viewport.
24. `[ ]` Stacking contexts.
25. `[ ]` z-index.
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
