# CLAUDE.md

Guidance for Claude Code (and any other agent) working in this repo.

## Project naming — unresolved inconsistency

The project is referred to by **three different names** across the repo, not yet unified:

- `README.md` title: **idleGo**
- `mockup/browser-idle-spec.md` title: **IdleBrowser**
- `mockup/Nimble Browser.dc.html` (product mockup) branding: **Nimble**
- `mockup/github.md` refers to a file named "Idle Labs Browser.dc.html", which does not match the actual `Nimble Browser.dc.html` in the repo (flagged AMBIGUOUS by graphify — confidence 0.25, likely stale doc)

Do not assume one name is canonical. If you need to pick one for new code/docs, ask the user first — this has not been decided.

## Source of truth for architecture

`mockup/browser-idle-spec.md` is the technical execution spec (Rust workspace, crate breakdown, phased roadmap). `mockup/Nimble Browser.dc.html` is the UI/UX mockup (interactive HTML prototype) — it drives product features, not implementation.

The spec has a **"Features do mockup (UI) — mapeamento pra spec"** section that cross-references every mockup feature against the spec/roadmap. Known gaps not yet covered by any crate/phase as of the last review:

- Automation scripting (user-defined JS: `pane.goto/fill/click`, `every()`, `on()`, cron triggers) — no crate owns this; closest candidates are `workers`/`js-runtime` but neither specs it out
- Resource monitor (per-profile CPU/RAM/FPS telemetry + UI)
- Workspaces (named groups of profiles) — directory scaffolded (`apps/shell/src/workspace`) but no functional spec
- Downloads & history UI per profile
- Interface i18n (EN/PT toggle)
- "Import from Chrome" onboarding flow — conflicts with the "no fingerprint spoofing" isolation requirement, needs a decision
- Settings: Performance throttling knobs, credential vault UX, dev tools panel

Two prior conflicts (proxy support, 5 vs 6 simultaneous accounts) were resolved in favor of the mockup — see Requisitos and the `net` crate row in the spec's crate table.

## Implementation status

- **Phase 1 (done)**: Cargo workspace scaffold at repo root (`apps/shell`, `xtask`, `crates/*`). Every crate is an empty compiling stub except `crates/dom`, which has a real arena-based DOM tree (generation-tagged `NodeId`, tested: create/append, reparent, remove + generation invalidation, subtree removal).
- **Phase 2 (in progress)**: `quickjs-sys` vendors QuickJS-ng v0.16.2 as a git submodule (`crates/js-runtime/quickjs-sys/vendor/quickjs-ng`) and compiles it via a `cc` build script; hand-written FFI bindings cover runtime/context lifecycle, `JS_Eval`, native-function registration (`JS_NewCFunction2` + context opaque data), and now custom classes (`JS_NewClassID`/`JS_NewClass`/`JS_NewObjectClass`/`JS_SetOpaque`/`JS_GetOpaque`) plus accessor properties (`JS_DefinePropertyGetSet` with `JS_CFUNC_GETTER`/`JS_CFUNC_SETTER` cprotos). `js-runtime` wraps it in a safe `Runtime`/`Context` with `eval() -> Result<String, EvalError>`. `dom` gained `find_by_id`/`text_content`/`set_text_content`, now exposed to JS via a real `Node` class (a boxed `dom::NodeId` as the object's opaque data, freed by a `JS_NewClass` finalizer) and a global `document.getElementById(id)` returning `Node` instances with a live `textContent` getter/setter on `Node.prototype` — see `crates/js-runtime/src/dom_bindings.rs`. The two placeholder globals this replaced (`__dom_get_text_by_id`/`__dom_set_text_by_id`) are gone. `performance.now()` is bound (per-process time origin — no document/navigation concept exists yet to give it a real per-document origin). `crypto.getRandomValues` is bound too, scoped to `Uint8Array` only, backed by `platform-apis::fill_random` (wraps the `getrandom` crate — OS CSPRNG). `document.visibilityState`/`document.hidden` are bound as static placeholders (always "visible"/false, since nothing tracks real pane focus yet — see `crates/js-runtime/src/page_visibility.rs`). `dom_bindings` and `page_visibility` share one `document` global via `crate::document::get_or_create()` (fixed after a merge had them clobbering each other). `Node.prototype` also has `addEventListener`/`removeEventListener`/`dispatchEvent` (`crates/js-runtime/src/events.rs`) — scoped to one listener per (node, type), and `dispatchEvent(type)` takes a plain string, not a real `Event` object. That closes every phase 2 roadmap item except timers/`requestAnimationFrame`, which need an event loop that isn't designed yet (deferred: belongs with `profile`/`ipc` in phase 4, where the per-tab process actually runs one).
- **Phase 3 (in progress)**: `css` has a real tokenizer (`lexer.rs`, a practical CSS Syntax Module Level 3 subset — no `url()`, no unicode-range), a selector/declaration parser (`parser.rs`, descendant combinator only, no `>`/`+`/`~`/pseudo-classes/attribute selectors, declaration values kept as raw tokens), and selector matching + cascade ordering (`cascade.rs`). `layout-engine` now depends on `css`+`dom` and does 3 things: `style.rs` resolves cascaded declarations into a typed `ComputedStyle` (`display`/`width`/`height`/`margin`/`padding`, no inheritance, no flex/positioning properties yet); `tree.rs` builds a `LayoutBox` tree by walking a live `dom::Dom` (only `Element` nodes get boxes, `display:none` prunes the subtree, no inline text layout — that needs the text-shaping crate); `layout.rs` computes `Dimensions` (block formatting context: no inline flow, no floats, no `position`, no margin collapsing) and dispatches per-box to `flex.rs` when `display: flex` — single-line flex only (no wrap/order/align-self/gap/shorthand), row and column direction, `flex-grow`/`flex-shrink` distribution, `justify-content`, `align-items`; tested including nested flex-in-flex. `flex-basis: auto` falls back to width/height then `0` — no content-based sizing exists anywhere in this engine. `layout-engine` also has `background-color`/`background` (solid color only: named colors, `#rgb`/`#rrggbb` hex). `render` now does real GPU work: `display_list.rs` flattens a `LayoutBox` tree into paint-ordered `Rect`s (skips transparent boxes), `gpu.rs`'s `GpuRenderer` rasterizes them via `wgpu` 0.20 to an off-screen texture and reads pixels back (headless, no window/surface — tested by asserting actual readback pixel values against a real adapter on this machine). One draw call, solid-color quads, straight alpha blending, no borders/images/text/shadows/clipping. `webgl` implements `create_shader`/`create_program`/`draw_triangles` (mirroring `gl.compileShader`/`gl.linkProgram`/`gl.drawArrays(TRIANGLES)`) over `wgpu`, using naga's GLSL frontend for *real* shader compilation — but naga's GLSL frontend only accepts **desktop** GLSL (`#version 450 core`), not GLSL ES, so shader source here isn't the GLSL ES actual WebGL requires (documented prominently in `crates/webgl/src/context.rs`, since it undercuts the crate's own name — no fix attempted, would need a different frontend or `300 es`→something-naga-accepts preprocessing). Headless texture + pixel readback, same pattern as `render::gpu`; no textures/uniforms/indices/framebuffers/VAOs/persistent bind state. `render` also has `Canvas2D` (`canvas.rs`): `fillRect`/`clearRect`/`getImageData` over a persistent accumulating texture (unlike `gpu.rs`'s one-shot render), starts fully transparent, `clearRect` uses a separate REPLACE-blend pipeline so it actually erases instead of no-op-blending transparent onto opaque. Solid-color rects only, no paths/strokes/text/images/gradients/transforms. That closes the spec's phase 3 roadmap line (CSS + Canvas 2D + WebGL) — all three real, all narrowly scoped.

Text rendering (previously "not started") is now real end to end: `layout-engine` gained `font-size`/`color` as its first *inherited* properties (`resolve_style` seeds from the parent's resolved value, not the CSS initial value), `dom::NodeData::Text` nodes produce a leaf `LayoutBox` (`text: Some(string)`) instead of being skipped, and `layout::layout_block` short-circuits text boxes to `layout_text_box`, which shapes/line-wraps via real `cosmic-text` (`text.rs`) against the containing width — actual line breaking, not a heuristic. `text.rs` also rasterizes glyphs to alpha-coverage bitmaps via `SwashCache` (`rasterize_glyph`), matching cosmic-text's own `PhysicalGlyph`/`with_pixels` convention. `render` composites them (`text.rs`'s `composite_glyphs`, CPU-side alpha blending, not a GPU glyph-atlas pipeline) onto the same pixel buffer `gpu`/`canvas` use, tested end to end from `dom` through to an actual non-transparent painted pixel. No general inline formatting context: a text node flows as a plain child of a block element only — text mixed with inline element siblings on the same line (`<p>hello <b>world</b></p>`) isn't supported.
- **Phase 4 (started)**: `ipc` has a real shared-memory double-buffered frame transport (`framebuffer.rs`, via the `shared_memory` crate) — one atomic `ready_index` picks which buffer is safe to read; the writer only ever writes the *other* one, so a reader can never observe a torn frame (resolves the spec's own "pendente: definir sincronização" note in favor of lock-free double buffering). `profile` spawns a real child OS process (`profile-worker` binary, `std::process::Command`) that hosts dom+css+layout-engine+render and publishes frames over that transport; `Profile` (host side) does `ping`/`reload`/`latest_frame`, and `quit()`+`Drop` guarantee a profile process is killed even if it hangs or the handle is dropped without `quit()` — the actual crash-isolation/security boundary, not just a logical one. No real page loading: `profile-worker` renders one hardcoded demo DOM tree, since there's still no HTML parser anywhere in the workspace (`html` crate is an empty stub) — that's the next real blocker for `profile` to matter beyond proving the transport. No input/navigation commands yet either (stdin protocol is just `PING`/`RELOAD`/`QUIT`). `apps/shell` doesn't drive any of this yet — still the phase-1 stub window.
- Validate with `cargo build --workspace` and `cargo test --workspace` after any change.
- **Windows build requirement**: `quickjs-sys`'s C compilation needs a Developer Command Prompt / `vcvars64.bat` environment — on this machine cc-rs's own MSVC autodetection does not locate the toolchain/SDK on its own (VS install and Windows SDK are on different drives). Also needs `/std:c11` (set in `build.rs`) for quickjs.c's C11 atomics; plain `/experimental:c11atomics` alone is not enough. If `cargo build` fails with a missing `stdlib.h` or a `C atomics require C11` error, that's this — run from `vcvars64.bat`, not a bug in the crate.

## Conventions

- Commits: Conventional Commits, one crate/domain per commit (`feat(dom): ...`, `chore(scaffold): ...`, `docs: ...`).
- All code, docs, and commit messages: English.
- Never commit `graphify-out/` (gitignored — contains absolute local filesystem paths).
- Tests are integration-style, not inline `#[cfg(test)] mod tests` in `src/`: put them under `crate/tests/<file>_test.rs` (e.g. `crates/dom/tests/dom_test.rs`). Only works cleanly when the tests exercise the crate's public API — if a test needs a private item, that's a signal to reconsider what's private, not to fall back to an inline module.

## Knowledge graph

This repo has a graphify knowledge graph (`graphify-out/`, gitignored — local only, rebuild with `/graphify` or `/graphify --update`). Re-run `--update` after any significant change so the graph stays current; it's the fastest way to answer "where is X" / "what references Y" without re-reading the whole repo.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
