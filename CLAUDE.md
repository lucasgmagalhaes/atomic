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

`mockup/browser-idle-spec.md` is the technical execution spec (Rust workspace, crate breakdown, phased roadmap). `mockup/Nimble Browser.dc.html` is the UI/UX mockup (interactive HTML prototype) — it drives product features, not implementation. `mockup/rendering-engine-gaps.md` is a detailed, code-verified gap map of the rendering engine specifically (css/layout-engine/render/webgl/dom) — what's real vs missing at the CSS-property/API level, not the crate/phase level `browser-idle-spec.md` tracks.

**`JS_ENGINE_CAPABILITY_MATRIX.md` is the source of truth for granular JS-engine status** — a checklist (done/needed, with file references) of the JS runtime, DOM classes/properties/functions, events, CSS/layout, paint/compositing, browser Web API classes, and navigation/security/lifecycle. Check it (and keep it updated) instead of asking "is X implemented" from scratch or re-deriving it from source; the "Implementation status" section below deliberately does not duplicate what it already tracks.

The spec has a **"Features do mockup (UI) — mapeamento pra spec"** section that cross-references every mockup feature against the spec/roadmap. As of the last review, every mockup feature has a real implementation somewhere in the workspace (automation scripting, resource monitor, workspaces, downloads/history UI, i18n, add-profile modal, Chrome import, performance settings incl. GPU adapter selection, credential vault with OS keychain). The one deliberate non-goal: a dev tools panel — this engine has no inspector/devtools protocol anywhere to back one, and none is planned.

Two prior conflicts (proxy support, 5 vs 6 simultaneous accounts) were resolved in favor of the mockup — see Requisitos and the `net` crate row in the spec's crate table.

## Implementation status

Phased roadmap, condensed — see git history / `graphify query` for the play-by-play of how each piece landed.

JS-engine specifics (runtime, DOM, events, CSS/layout, paint, Web API classes) live in `JS_ENGINE_CAPABILITY_MATRIX.md`, not here. What follows is the surrounding, non-JS-engine infrastructure the matrix doesn't track:

- **Phase 1 (done)** — Cargo workspace scaffold (`apps/shell`, `xtask`, `crates/*`).
- **Phases 2–3 (done)** — `quickjs-sys` vendors QuickJS-ng as a git submodule, compiled via a `cc` build script. `css`/`layout-engine`/`render`/`webgl` form the rendering pipeline — see the matrix for capability detail.
- **Phase 4 (in progress)** — the bulk of the engine:
  - `ipc`: lock-free double-buffered shared-memory frame transport.
  - `profile`/`profile-worker`: spawns a sandboxed child process per tab running a real ~60Hz vsync loop, with `NAVIGATE`/`RELOAD`/`PING`/`QUIT`/`CLICK`/`FILL`/`EVAL` over stdin. Fetches real HTML/CSS (`<link>`, `@import`, `@media`), sends/stores cookies, and threads per-profile proxy/DNS/GPU-adapter choice through every network call.
  - `html`: `html5ever::TreeSink` over `dom::Dom` — real tag-soup recovery; doctype/PI/`<template>`/foreign-content are documented scope cuts (no `dom` node variant for them).
  - `net`: HTTP/TLS GET (`hyper`+`rustls`), per-request proxy (`CONNECT` tunneling, TLS-through-tunnel, optional Basic auth), custom DNS-over-UDP resolution, file downloads, and full response headers for cookie handling. No redirects/pooling/WebSocket.
  - `workers`: real OS-thread Web Workers, one `Runtime`/`Context` each — genuine parallelism, not cooperative.
  - `storage`: `localStorage`/`sessionStorage`, real `Set-Cookie`/cookie-jar parsing (incl. `SameSite`, RFC 6265 domain/path matching), and a scoped-down IndexedDB (object stores + one secondary index, cursors, no versioned schema/async transactions). Structured-clone-shaped values via a hand-rolled JSON-like wire format.
  - `security`: AES-256-GCM vault (with real Windows Credential Manager backing; Linux/macOS keychains are documented `Unsupported`), Windows process sandboxing (Job Objects; Linux/macOS sandboxing not implemented), and a signed updater (ed25519 manifest signing + SHA-256 verification — no update feed/delta-patching/rollback).
  - `platform-apis`: real per-process CPU/RAM sampling (Windows-only) and OS clipboard read/write.
  - `apps/shell`: GUI wired to the real render stack (one `profile-worker` per pane), workspaces (create/switch/move, background-throttled hidden panes), downloads/history panels (persisted per stable pane identity), i18n (EN/PT), add-profile modal, Chrome import (bookmarks/history/cookies/passwords — `v20`/App-Bound Encryption not supported), Settings (max panes, FPS cap, GPU adapter dropdown, OS-keychain toggle), and the automation engine kept alive across the app's lifetime via `AutomationEngine::rebind`.

### Known gotchas (recurring bug classes — read before touching `js-runtime`'s native bindings)

- **quickjs class IDs are per-`Runtime`, not global.** `crates/js-runtime/src/class_registry.rs` keys a process-wide map by `(runtime pointer, class-kind name)` so every runtime gets a collision-free ID; `Runtime::drop` evicts its entries before the OS can reuse that address. An earlier design shared one static `AtomicU32` class ID across every `Runtime` in the process and caused real, reproducible UB under concurrent `Runtime` construction.
- **A class's *prototype* is per-`Context`, not per-`Runtime`.** `JS_GetClassProto`/`JS_SetClassProto` operate on `ctx->class_proto[class_id]`. An `ensure_*_class` function must check whether a prototype already exists (`JS_GetClassProto` returns `JS_TAG_OBJECT`) before rebuilding it — the un-set default is `JS_TAG_NULL`, not `JS_TAG_UNDEFINED`. Skipping this check let a chain of nested "ensure parent class" calls silently rebuild the whole `Node`→`Element`→`HTMLElement` hierarchy on every subclass registration, orphaning earlier objects from the prototype chain the global constructors ended up exposing (`instanceof HTMLInputElement` true, `instanceof Node` false).
- **`JS_GetException`/`JS_GetGlobalObject` return owned references.** Always capture and `JS_FreeValue` them explicitly — several real refcount leaks in this codebase (Response/Request `.json()`'s invalid-JSON path, `navigator.clipboard`'s reject path) trace back to discarding the result inline, and only surface later as quickjs's own `list_empty(&rt->gc_obj_list)` assertion on shutdown.
- **Never re-entrantly `JS_Eval` a small script from inside a native callback** (e.g. building a `"JSON.parse(...)"` source string to parse JSON from Rust) — it was a real, rare source of memory corruption. Use the dedicated C API instead (`JS_ParseJSON`, bound in `quickjs-sys`).
- **Windows build requirement**: `quickjs-sys`'s C compilation needs a Developer Command Prompt / `vcvars64.bat` environment (cc-rs's own MSVC autodetection may not find the toolchain/SDK) plus `/std:c11` for quickjs.c's C11 atomics.
- Validate with `cargo build --workspace` and `cargo test --workspace` after any change (add `--exclude automation` if that crate is mid-edit).

## Conventions

- Commits: Conventional Commits, one crate/domain per commit (`feat(dom): ...`, `chore(scaffold): ...`, `docs: ...`).
- All code, docs, and commit messages: English.
- Indentation is 4 spaces (`rustfmt.toml`'s `tab_spaces`, `.editorconfig`'s `indent_size`) — run `cargo fmt --all` after editing.
- Never commit `graphify-out/` (gitignored — contains absolute local filesystem paths).
- Tests are integration-style, not inline `#[cfg(test)] mod tests` in `src/`: put them under `crate/tests/<file>_test.rs` (e.g. `crates/dom/tests/dom_test.rs`). Only works cleanly when the tests exercise the crate's public API — if a test needs a private item, that's a signal to reconsider what's private, not to fall back to an inline module.
- Formatting is enforced on commit via a git pre-commit hook (`cargo fmt -- --check`), installed by `cargo-husky` (a dev-dependency of `xtask` — the Rust-native equivalent of Husky, since this workspace has no Node/npm anywhere). The hook installs itself into `.git/hooks/pre-commit` the first time `cargo test`/`cargo test -p xtask` runs after a fresh clone; run `cargo fmt --all` to fix a failing check, or `git commit --no-verify` to bypass in a pinch.

## Knowledge graph

This repo has a graphify knowledge graph (`graphify-out/`, gitignored — local only, rebuild with `/graphify` or `/graphify --update`). Re-run `--update` after any significant change so the graph stays current; it's the fastest way to answer "where is X" / "what references Y" without re-reading the whole repo.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
