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
- **Phase 2 (in progress)**: QuickJS embedded via FFI + minimal DOM↔JS bindings, per the spec's crate table (`js-runtime`, `js-runtime/quickjs-sys`).
- Validate with `cargo build --workspace` and `cargo test -p dom` after any change.

## Conventions

- Commits: Conventional Commits, one crate/domain per commit (`feat(dom): ...`, `chore(scaffold): ...`, `docs: ...`).
- All code, docs, and commit messages: English.
- Never commit `graphify-out/` (gitignored — contains absolute local filesystem paths).

## Knowledge graph

This repo has a graphify knowledge graph (`graphify-out/`, gitignored — local only, rebuild with `/graphify` or `/graphify --update`). Re-run `--update` after any significant change so the graph stays current; it's the fastest way to answer "where is X" / "what references Y" without re-reading the whole repo.
