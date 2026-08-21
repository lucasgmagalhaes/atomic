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
- **Phase 2 (in progress)**: `quickjs-sys` vendors QuickJS-ng v0.16.2 as a git submodule (`crates/js-runtime/quickjs-sys/vendor/quickjs-ng`) and compiles it via a `cc` build script; hand-written FFI bindings cover runtime/context lifecycle, `JS_Eval`, native-function registration (`JS_NewCFunction2` + context opaque data), and now custom classes (`JS_NewClassID`/`JS_NewClass`/`JS_NewObjectClass`/`JS_SetOpaque`/`JS_GetOpaque`) plus accessor properties (`JS_DefinePropertyGetSet` with `JS_CFUNC_GETTER`/`JS_CFUNC_SETTER` cprotos). `js-runtime` wraps it in a safe `Runtime`/`Context` with `eval() -> Result<String, EvalError>`. `dom` gained `find_by_id`/`text_content`/`set_text_content`, now exposed to JS via a real `Node` class (a boxed `dom::NodeId` as the object's opaque data, freed by a `JS_NewClass` finalizer) and a global `document.getElementById(id)` returning `Node` instances with a live `textContent` getter/setter on `Node.prototype` — see `crates/js-runtime/src/dom_bindings.rs`. The two placeholder globals this replaced (`__dom_get_text_by_id`/`__dom_set_text_by_id`) are gone. `performance.now()` is bound (per-process time origin — no document/navigation concept exists yet to give it a real per-document origin). `crypto.getRandomValues` is bound too, scoped to `Uint8Array` only, backed by `platform-apis::fill_random` (wraps the `getrandom` crate — OS CSPRNG). Still open from the phase 2 roadmap line: `addEventListener`, timers, `requestAnimationFrame`, Page Visibility — timers/rAF specifically need an event loop, which isn't designed yet (deferred: it belongs with `profile`/`ipc` in phase 4, where the per-tab process actually runs one).
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
