# idleGo

A free and open source browser focused on managing idle games. Also referred to as **Nimble** in the product mockup (`mockup/`) — naming is not yet unified across README/spec/mockup.

## Goal

Run several isolated game accounts side by side (tiled panes, per-account session isolation, optional proxy, automation scripts) without the overhead of embedding a full browser engine like Chromium.

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

## Status

Phase 1 in progress. `crates/dom` has a real implementation with unit tests; every other crate is a compiling stub.

```bash
cargo build --workspace
cargo test -p dom
```
