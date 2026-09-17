# CLAUDE.md

Guidance for Claude Code (and any other agent) working in this repo.

## Project naming (decided 2026-09-12)

The project is named **Atomic**. It previously went by three different, never-unified names across the repo (`README.md`'s "idleGo", `mockup/browser-idle-spec.md`'s "IdleBrowser", and the mockup/UI's own "Nimble" branding) — all three, plus every internal identifier that used one of them (the `NimbleApp` struct, the `window.nimble` JS bridge namespace, `nimble-*` shared-memory/temp-dir/vault path prefixes, etc.), were renamed to Atomic in one pass. If you find a leftover "idleGo"/"IdleBrowser"/"Nimble"/"nimble" anywhere, it's a miss from that rename, not a live naming decision — fix it to Atomic rather than treating it as an open question.

## Engine pivot (decided 2026-09-16)

**Radical change, supersedes everything below that assumed a custom engine.** This project no longer builds its own rendering/JS engine. It now wraps **Chromium via CEF** (Chromium Embedded Framework) and focuses on what CEF doesn't give for free: aggressive default isolation between profiles, fast cold start, and low idle memory/CPU at scale (many profiles/tabs open at once). Decision: **CEF** (not WebView2/system-Edge, not a from-scratch engine like Servo) — chosen for cross-platform reach and fine-grained control over process model/flags, which the isolation and startup goals both depend on.

Consequence for the workspace: **`crates/neutron` (all 11 sub-crates + facade) and `crates/atomicjs` are being deleted**, not archived — the whole point of the pivot is that a consolidated engine replaces them, there's nothing worth keeping in parallel. Their spec is preserved for historical reference at [`spec-archive-neutron-engine/`](spec-archive-neutron-engine/INDEX.md) but is dead, not a roadmap. See [`spec/ROADMAP.md`](spec/ROADMAP.md) for the phased removal + CEF-integration plan (P0 = delete neutron/atomicjs and land a CEF skeleton) and [`spec/architecture/`](spec/architecture) for the CEF process model and isolation/perf design. Until P0 lands, `crates/atomic` still references `neutron` in ~16 files (`chrome_engine/*`, `chrome_bridge/*`, `automation_bridge.rs`, `browser_view.rs`, `app/*`, `settings.rs`) — these are known, tracked, not a separate problem to solve ad hoc.

The former "Three-scope encapsulation" rule (`crates/atomic` depends on the `neutron` facade only, never its sub-crates, enforced by `cargo run -p xtask -- check-neutron-boundary`) is retired along with `neutron` itself — remove that xtask command and its pre-commit hook wiring as part of P0, don't keep enforcing a boundary around a deleted crate.

## Product positioning (updated 2026-09-16, originally decided 2026-08-26)

The core differentiator is still **many isolated profiles running light at once** — that hasn't changed. What changed is how it's achieved: previously "per-profile process isolation with a lightweight custom engine vs. Chromium's heavier baseline"; now "per-profile isolation and aggressive idle/startup optimization *on top of* Chromium itself" (see [`spec/architecture/isolation-and-perf.md`](spec/architecture/isolation-and-perf.md)). Rendering/JS compatibility is no longer a competitive axis at all — Chromium already wins that outright, for free. Don't market on raw rendering speed (everyone has the same engine now); the pitch is entirely the many-isolated-profiles economics: startup time, idle memory/CPU, and default isolation.

## Source of truth for architecture

`mockup/browser-idle-spec.md` and `mockup/Atomic Browser.dc.html` predate the CEF pivot and describe the old custom-engine product/UI vision — still useful for product-feature intent (workspaces, profile management UI, automation, i18n, etc., all of which carry over unchanged), **not** for engine/rendering implementation detail, which is now moot. `mockup/rendering-engine-gaps.md` is entirely about the deleted `neutron` engine — historical only.

**`spec/` is the source of truth for the CEF pivot's architecture and rollout plan.** Start at `spec/INDEX.md`, then `spec/ROADMAP.md` to pick a task, then `spec/RULES.md` before writing code. `spec/architecture/cef-integration.md` covers the CEF process model and the Rust↔C-API boundary; `spec/architecture/isolation-and-perf.md` covers the per-profile isolation and startup/idle-footprint design — the two documents that matter most for this pivot. The old JS-engine matrix/architecture docs (per-API done/needed checklists, quickjs-specific playbook) live at `spec-archive-neutron-engine/` for reference only — don't pick up tasks from there.

## Implementation status (pre-pivot, being replaced — see spec/ROADMAP.md)

**This whole section describes the custom-engine architecture being deleted (Engine pivot, above).** Kept here only until `spec/ROADMAP.md` P0 lands, so an agent mid-migration can still see what a given crate used to do before its CEF replacement exists. Do not build new features against this architecture — new work goes through `spec/ROADMAP.md`'s CEF phases instead.

Phased roadmap, condensed — see git history / `graphify query` for the play-by-play of how each piece landed.

JS-engine specifics (runtime, DOM, events, CSS/layout, paint, Web API classes) lived in `spec-archive-neutron-engine/` (start at its `INDEX.md`), historical only now. What follows is the surrounding, non-JS-engine infrastructure the matrix didn't track — most of it (`ipc`, `security`'s sandboxing, `platform-apis`) is engine-agnostic and carries over into the CEF architecture unchanged; `profile`/`profile-worker`, `net`, `storage`, `html`, `workers` are being replaced by CEF's own equivalents (see `spec/ROADMAP.md` P1):

- **Phase 1 (done)** — Cargo workspace scaffold (`crates/atomic`, `xtask`, `crates/*`).
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
  - `crates/atomic`: GUI wired to the real render stack (one `profile-worker` per pane), workspaces (create/switch/move, background-throttled hidden panes), downloads/history panels (persisted per stable pane identity), i18n (EN/PT), add-profile modal, Chrome import (bookmarks/history/cookies/passwords — `v20`/App-Bound Encryption not supported), Settings (max panes, FPS cap, GPU adapter dropdown, OS-keychain toggle), and the automation engine kept alive across the app's lifetime via `AutomationEngine::rebind`.

### Known gotchas (pre-pivot, `js-runtime`/quickjs-specific — historical, see spec/architecture/cef-integration.md for the CEF-era equivalent)

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
- Formatting is enforced on commit via a git pre-commit hook, installed by `cargo-husky` (a dev-dependency of `xtask` — the Rust-native equivalent of Husky, chosen back when this workspace had no Node/npm anywhere; that's no longer true — see below — but the hook itself stays cargo-husky-based, no reason to migrate it) from the custom script at `.cargo-husky/hooks/pre-commit` (the `user-hooks` cargo-husky feature, not its fixed `run-cargo-fmt` one — this script needs to run more than one checker). The hook installs itself into `.git/hooks/pre-commit` the first time `cargo test`/`cargo test -p xtask` runs after a fresh clone. It runs two checks:
  - `cargo fmt --all -- --check` for every Rust crate — run `cargo fmt --all` to fix a failing check.
  - `dprint check` for the workspace's non-Rust source, `crates/atomic/chrome-ui/**/*.{html,css,js}` (config: `dprint.json`, excluding `node_modules/` — see that file's own `excludes`) — run `dprint fmt` to fix a failing check. Requires `dprint` on `PATH` (`cargo install dprint`) and a one-time `dprint config add css markup typescript` to populate `dprint.json`'s plugin list (see that file's own comment for why it isn't pre-filled); the hook only warns and skips this half if `dprint` isn't installed, so a fresh clone without it still gets Rust formatting enforced.
  - `git commit --no-verify` bypasses both in a pinch.
- **`crates/atomic/chrome-ui/` is a real npm project** (2026-09-16, `spec/architecture/chrome-ui.md`'s CEF-rendered shell UI) — the one deliberate, scoped exception to this workspace otherwise having no Node/npm: `package.json`/`package-lock.json` are tracked, `node_modules/` is gitignored like any other npm project. Real dependencies (Preact, `htm`), not hand-downloaded/vendored files — install with `npm install` from that directory after a fresh clone before `cef_profile_worker` can load `toolbar.html` (it references `node_modules/preact/...`/`node_modules/htm/...` directly via `<script src>`, no bundler). Don't add a bundler/build step without updating this note and the pre-commit hook.

## Knowledge graph

This repo has a graphify knowledge graph (`graphify-out/`, gitignored — local only, rebuild with `/graphify` or `/graphify --update`). Re-run `--update` after any significant change so the graph stays current; it's the fastest way to answer "where is X" / "what references Y" without re-reading the whole repo.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
