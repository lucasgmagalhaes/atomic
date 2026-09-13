# Plan: Text Selection — Real `Range`/`Selection` API (ROADMAP item 20)

**Source**: `spec/ROADMAP.md` item 20's own "Still `[ ]`" note ("text selection", the one remaining sub-scope of Default actions — button/input submit/reset, `<a href>` click navigation, and Tab/Shift+Tab focus navigation are all already real)
**Complexity**: Medium

## Summary

This engine has zero `Selection`/`Range` API today (confirmed by grep: no `getSelection`, no `Range` anywhere). Add the real, script-driven core of both: `document.createRange()` returns a real `Range` (`startContainer`/`startOffset`/`endContainer`/`endOffset`/`collapsed`, `setStart`/`setEnd`/`selectNodeContents`/`collapse`, and a real `toString()` that concatenates actual document text between the two boundary points via a pre-order text-node walk), and `window.getSelection()`/`document.getSelection()` return a real, identity-stable `Selection` (`addRange`/`removeAllRanges`/`getRangeAt(0)`/`rangeCount`/`toString()`/`collapse`/`anchorNode`/`anchorOffset`/`focusNode`/`focusOffset`).

Scoped narrower than the full spec, documented explicitly: **no real user-driven mouse-drag/keyboard text selection** — that would need a new host input command in `profile-worker` (a real drag-to-select interaction loop), a materially larger and separate follow-up not requested here. Only the script-driven API surface (`createRange`, `getSelection`, and everything callable on the objects they return) is real in this pass. Range boundary points are scoped to **text-node containers only** — a real, common, well-defined case; an element-node container (spec's child-index-based offset) isn't modeled, matching this crate's existing "real but narrower" precedent (e.g. `crypto.getRandomValues`'s `Uint8Array`-only scope).

## Patterns to Mirror

| Category | Source | Pattern |
|---|---|---|
| Plain-object class with hidden state, no native quickjs class | `crates/js-runtime/src/abort_controller/` (`controller.rs`, `signal.rs`, `helpers.rs`) | `JS_NewObject` + `resolve_prototype` (constructor's own `.prototype`, same helper) + hidden `__`-prefixed own properties (`get_prop`/`set_prop`) for state — `Range`/`Selection` need exactly this shape (a `NodeId` stored as `js_float64`, same convention `MessagePort`'s own `__port_id` already uses). |
| `NodeId` ↔ real JS `Node` object | `crates/js-runtime/src/dom_bindings/node_registry.rs`'s `node_object`/`node_id`/`node_class_id_for`, used by `navigation.rs`'s `navigation_node` | Exact reverse/forward conversion `Range.startContainer`/`endContainer` getters and `setStart`/`setEnd`'s argument parsing both need. |
| Global constructor + prototype registration | `crates/js-runtime/src/abort_controller/mod.rs`'s `register` | Same `JS_NewObject` proto + `JS_NewCFunction2(..., JS_CFUNC_CONSTRUCTOR_OR_FUNC, ...)` + `JS_SetPropertyStr(ctor, "prototype", proto)` + `JS_SetPropertyStr(global, name, ctor)` shape, called from `context/mod.rs`'s `register_standard_globals` (where `abort_controller::register(ptr)` is already called). |
| Per-context object-identity cache | `crates/js-runtime/src/window_registry.rs`'s `REMOTE_WINDOW_OBJECTS` | `window.getSelection() === window.getSelection()` must hold (same real spec identity-stability requirement `iframe.contentWindow` already had to solve this session) — a thread-local `HashMap<ctx_ptr, JSValue>` singleton per context, lazily built on first access. |
| Pre-order tree walk using public `Node` fields | `dom::Dom`'s `Node { id, data, parent, children }` (all `pub`) | `Range::toString()`'s real text-concatenation walk — no new `dom` API needed, a plain recursive walk from `dom.root()` using `dom.get(id)`/`node.children`. |
| Tests | `crates/js-runtime/tests/message_channel_test.rs`, `abort_controller`-adjacent tests | `Context::eval`-based assertions on real DOM text + real `Range`/`Selection` method calls. |

## Files to Change

| File | Action | Why |
|---|---|---|
| `crates/js-runtime/src/selection/mod.rs` | CREATE | Public `register(ctx)` entry point — registers both `Range` and `Selection` global constructors/prototypes, mirroring `abort_controller::register`'s shape. |
| `crates/js-runtime/src/selection/helpers.rs` | CREATE | Shared `get_prop`/`set_prop`/`resolve_prototype`/hidden-property-name constants — same content shape as `abort_controller/helpers.rs`, kept as its own copy per this crate's existing "small per-module copy, not a shared utility" convention. |
| `crates/js-runtime/src/selection/range.rs` | CREATE | `Range` — constructor (`new Range()`/`document.createRange()`), `setStart`/`setEnd`/`selectNodeContents`/`collapse`, `startContainer`/`startOffset`/`endContainer`/`endOffset`/`collapsed` getters, `toString()`. |
| `crates/js-runtime/src/selection/selection_obj.rs` | CREATE | `Selection` — `addRange`/`removeAllRanges`/`getRangeAt`/`rangeCount`/`toString`/`collapse`/`anchorNode`/`anchorOffset`/`focusNode`/`focusOffset` getters; the per-context singleton cache. |
| `crates/js-runtime/src/lib.rs` | UPDATE | `mod selection;`. |
| `crates/js-runtime/src/context/mod.rs` | UPDATE | `register_standard_globals` calls `selection::register(ptr)`, alongside the existing `abort_controller::register(ptr)`. |
| `crates/js-runtime/src/window.rs` | UPDATE | Add `getSelection` as a real global method (delegates to `selection`'s singleton-cache accessor). |
| `crates/js-runtime/src/dom_bindings/document.rs` (or wherever `document`'s own methods are installed) | UPDATE | Add `document.getSelection()` — same delegation, same returned object identity as `window.getSelection()`. |
| `crates/js-runtime/tests/text_selection_test.rs` | CREATE | Real `Range`/`Selection` behavior: same-text-node substring, cross-node concatenation, `collapsed`, `Selection.toString()` delegating to its range, `addRange`/`removeAllRanges`, object-identity stability of `getSelection()`. |
| `spec/ROADMAP.md` | UPDATE | Flip item 20's "text selection" sub-note to done, with the real scope cut (script-driven only, text-node containers only). |

## Tasks

### Task 1: `Range` — boundary points + `collapsed`
- **Action**: `range.rs`'s constructor builds a plain object with hidden `__start_node` (a real `Node` object, dup'd)/`__start_offset`/`__end_node`/`__end_offset`, all initially pointing at `(document, 0)` (matching a real freshly-created `Range`, collapsed at the document start). `setStart(node, offset)`/`setEnd(node, offset)` validate `node` is a real text node (via `node_id` + `dom.get(id)` checking `NodeData::Text`) before storing — a non-text-node argument is a documented no-op (real spec would throw `InvalidNodeTypeError`; this crate's own convention elsewhere is to no-op on out-of-scope input rather than reject with a full spec-shaped exception, e.g. `crypto.getRandomValues`'s non-`Uint8Array` case). `selectNodeContents(node)` sets both boundaries to `(node, 0)`/`(node, text.len())` for a text node. `collapse(toStart)` sets the non-anchor boundary equal to the anchor one. `collapsed` getter: real node-and-offset equality check.
- **Mirror**: `abort_controller/signal.rs`'s own hidden-property read/write shape exactly.
- **Validate**: `cargo build -p js-runtime` (behavior proven in Task 4's tests).

### Task 2: `Range::toString()` — real cross-node text concatenation
- **Action**: A pre-order walk from `dom.root()` collecting every `NodeData::Text` node's `(NodeId, String)` in document order (a small local helper, not a new `dom` API — everything needed is already `pub`). Then a single linear scan: accumulate text once the walk reaches the start container (sliced from `start_offset`), continue accumulating full text node contents until the end container is reached (sliced up to `end_offset`), stop there. The same-container case (`start_container == end_container`) is the common single-slice special case of this general algorithm, not a separate code path.
- **Mirror**: No direct precedent in this crate (first document-order tree walk of this shape) — grounded instead in `dom::Dom`'s already-`pub` `Node.children`/`.parent` fields, same access level every other `dom_bindings` submodule already uses.
- **Validate**: `cargo test -p js-runtime --release --test text_selection_test -- --test-threads=1` (same-node and cross-node cases).

### Task 3: `Selection` — real singleton, `addRange`/`removeAllRanges`/accessors
- **Action**: A thread-local `HashMap<usize, sys::JSValue>` (ctx pointer → the one `Selection` object for that context), lazily built on first `getSelection()` call — mirrors `window_registry::REMOTE_WINDOW_OBJECTS`'s exact identity-cache shape (dup on cache hit). The `Selection` object itself holds `__range: Option<Range-shaped-object>` (`JS_TAG_UNDEFINED` sentinel for "no range", matching real `rangeCount === 0`). `addRange(range)` stores it (real single-range browser behavior — replaces any existing one, doesn't append, matching modern Chromium/Firefox, not the legacy multi-range model); `removeAllRanges()` clears it; `getRangeAt(0)` returns it (any other index is out of range - documented no-op returning `undefined` rather than throwing `IndexSizeError`, same convention Task 1's `setStart` non-text-node case already established); `rangeCount` is `0`/`1`; `toString()` delegates to the stored range's own `toString()` (empty string if none); `collapse(node, offset)` builds and stores a real collapsed `Range` at that point; `anchorNode`/`anchorOffset`/`focusNode`/`focusOffset` mirror the stored range's start/end (a `Selection` built via `addRange`, not a real user drag, has no independent anchor/focus direction — matches this being the one real construction path in this pass's scope).
- **Mirror**: `window_registry.rs`'s `REMOTE_WINDOW_OBJECTS` cache + `make_remote_window`'s "check cache, dup on hit, build+cache on miss" shape.
- **Validate**: `cargo test -p js-runtime --release --test text_selection_test -- --test-threads=1`.

### Task 4: Wire `window.getSelection()`/`document.getSelection()` + tests
- **Action**: Both call the same `selection::get_or_create(ctx)` accessor, so `window.getSelection() === document.getSelection()` holds (real spec identity). New `text_selection_test.rs`: (a) `range.setStart/setEnd` on the same text node + `toString()` returns the real substring; (b) a range spanning two sibling text nodes (e.g. `<p>hello <b>world</b></p>`'s two text children) concatenates real text across both; (c) `collapsed` is `true` only when start equals end; (d) `getSelection().addRange(range); getSelection().toString()` matches the range's own `toString()`; (e) `getSelection() === getSelection()` (identity); (f) `removeAllRanges()` resets `rangeCount` to `0` and `toString()` to `""`.
- **Mirror**: `message_channel_test.rs`'s `Context::eval`-based assertion style.
- **Validate**: `cargo test -p js-runtime --release --test text_selection_test -- --test-threads=1`.

## Validation

```bash
cargo build -p js-runtime
cargo test -p js-runtime --release --test text_selection_test -- --test-threads=1
cargo build --workspace --exclude shell
cargo test --workspace --exclude shell --exclude automation --release -- --test-threads=1
cargo fmt --all
```

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| No real user-driven (mouse-drag/keyboard) selection — a page relying on the browser to *create* a `Selection` from user interaction sees an always-empty one | High (accepted) | Documented scope cut, matching the plan's own explicit framing: this pass makes the *API* real for script-driven construction, not the interaction loop. A future host input command is real, separate follow-up work. |
| Cross-node `toString()`'s pre-order walk starts from `dom.root()` every call — O(document size) per call, no incremental/cached traversal | Medium (accepted) | Matches this engine's existing "correctness over micro-optimization, revisit if it's ever the bottleneck" convention elsewhere (e.g. whole-tree layout rebuilds before item 13's cache existed) — a real page rarely calls `Range.toString()` in a hot loop. |
| `Range`/`Selection` objects holding a dup'd `Node` reference (`__start_node`, etc.) could leak if the object itself is garbage-collected without a finalizer freeing it | Medium | Since both are plain `JS_NewObject` instances (no native class/opaque data, matching `AbortController`/`AbortSignal`), quickjs-ng's own GC already frees every property (including the dup'd `Node` value) when the object itself is collected — no manual finalizer needed, same as `AbortSignal`'s own `__signal_listeners` array requires none. |
| Element-node range boundaries (spec's child-index offsets) silently no-op instead of throwing | High (accepted) | Documented scope cut, consistent with this crate's established "no-op on out-of-scope input" convention rather than a full spec-shaped exception surface. |

## Acceptance
- [ ] Task 1-4 complete, each independently testable
- [ ] `Range` real for text-node boundaries: `setStart`/`setEnd`/`selectNodeContents`/`collapse`/`collapsed`/`toString()` (same-node and cross-node)
- [ ] `Selection` real: `addRange`/`removeAllRanges`/`getRangeAt`/`rangeCount`/`toString`/`collapse`/`anchorNode`/`anchorOffset`/`focusNode`/`focusOffset`
- [ ] `window.getSelection() === document.getSelection()` (real object identity)
- [ ] Full workspace build + test green
- [ ] `spec/ROADMAP.md` item 20 updated with the real scope cut (script-driven only, text-node containers only, no user-interaction-driven selection)
