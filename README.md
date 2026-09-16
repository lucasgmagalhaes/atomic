# Atomic

A free and open source browser built from scratch around one core idea: run many isolated profiles side by side — light enough that having dozens open costs a fraction of what Chromium-based multi-profile tools cost. Also referred to as **Atomic** in the product mockup (`mockup/`) — naming is not yet unified across README/spec/mockup.

## Goal

Run several isolated profiles side by side (tiled panes, per-profile session isolation, optional proxy, automation scripts, resource monitoring) without the memory/CPU overhead of embedding a full browser engine like Chromium for every profile.

This started as a browser for managing idle games specifically (many accounts open at once, background-throttled) — that's still a real use case, but the actual product surface (per-profile process isolation, workspaces, credential vault, Chrome import, automation engine) is more general: **any workflow that needs many isolated browser sessions running light at once** — idle-game farming, multi-account management, or kiosk/embedded deployments where footprint is a hard constraint, not just a nice-to-have.

Full reasoning behind this positioning (market research, what this engine can and can't realistically compete with Chromium on) — [`spec/ROADMAP.md`](spec/ROADMAP.md)'s "Product scope decision" section.

## Architecture

Rendering engine built from scratch (no CEF/Chromium/Ultralight/Tauri) for lightness, full control, and cross-platform consistency — the only exception is TLS, which uses `rustls` rather than a custom implementation. Key choices:

- **JS**: QuickJS via C↔Rust FFI
- **Network/TLS**: `hyper` + `rustls`
- **HTML parsing**: `html5ever`
- **Renderer**: `wgpu` (Vulkan/Metal/DX12 abstraction)
- **Text**: `cosmic-text`
- **Shell UI**: `egui` + `eframe`
- **Account isolation**: one OS process per profile + IPC (crash isolation, security boundary)

Full rationale and crate breakdown: [`mockup/browser-idle-spec.md`](mockup/browser-idle-spec.md).

## Roadmap

| Phase | Deliverable |
|---|---|
| 1 | Workspace scaffold + real `dom` crate (arena-based, generation-tagged `NodeId`) |
| 2 | Embedded QuickJS + minimal DOM↔JS bindings |
| 3 | CSS (flex/positioning) + Canvas 2D + WebGL |
| 4 | Full storage + Workers + per-process isolation (profile/ipc) + complete shell + fetch/XHR/File/Blob |
| 5 | Hardening: per-platform process sandboxing, on-disk session encryption, signed updater |
| 6 | On-demand: Wake Lock, big-number formatting, ResizeObserver/IntersectionObserver, Service Worker/PWA, Gamepad, MutationObserver |

## Agent tooling (Claude Code)

This repo is worked on with [Claude Code](https://claude.com/claude-code), and two pieces of agent tooling are part of that workflow:

### ECC ([affaan-m/ECC](https://github.com/affaan-m/ECC))

A plugin bundle of agents, skills, hooks and rules for Claude Code (and other harnesses) — planning, code review, build-repair, and security agents, plus the plan → test → implement → review → verify loop this project's own contribution workflow follows. Recommended for anyone doing substantial work in this repo: it gives Claude Code the same reviewer/build-fixer agents already used when landing features here (see `spec/RULES.md` and this repo's own commit history for the workflow it enforces).

Install (either path installs the same `ecc@ecc` plugin — don't run both):

```bash
/plugin marketplace add https://github.com/affaan-m/ECC
/plugin install ecc@ecc
```

or, for a guided multi-harness setup (Claude Code, Codex, Kimi Code):

```bash
npx ecc-universal@2.2.1 setup
```

See the [ECC README](https://github.com/affaan-m/ECC) for the full agent/skill catalog and configuration options.

### impeccable

A frontend design skill (`shape`/`audit`/`critique`/`polish`/`harden`/... — see [`.agents/skills/impeccable/SKILL.md`](.agents/skills/impeccable/SKILL.md) for the full verb list) for UI work on `crates/atomic`'s `egui` interface and the mockup HTML prototype. It's vendored directly in this repo under `.agents/skills/impeccable/` (plus Codex agent definitions under `.codex/agents/`), so there's nothing to install — cloning the repo and opening it in Claude Code or Codex is enough; the skill is auto-discovered from the project directory. Invoke it with `/impeccable <verb> <target>` (e.g. `/impeccable audit crates/atomic`).

## Status

Well past the phase table above now — every crate listed under "Architecture" has a real, tested implementation (JS engine, DOM, CSS/layout, GPU render, networking/TLS, per-profile process isolation, workers, storage). See [`CLAUDE.md`](CLAUDE.md)'s "Implementation status" section for the current detail, and [`spec/INDEX.md`](spec/INDEX.md) for what's left on the JS-engine side specifically.

```bash
cargo build --workspace
cargo test --workspace
```
