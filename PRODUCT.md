# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

<!-- Note: this is inaccurate to the actual target — a native desktop app (egui/eframe + wgpu), not a browser-based web app. Impeccable's platform taxonomy only offers web/ios/android/adaptive; recorded as `web` per the schema default since none of the other three fit either, but future design work should treat this as a native desktop shell, not a webpage. -->

## Stack

Existing Rust workspace (not greenfield) — `apps/shell` (egui + eframe UI), `xtask`, `crates/*` (js-runtime, dom, css, layout-engine, render, webgl, ipc, profile/profile-worker, html, net, workers, storage, security, platform-apis). See [CLAUDE.md](CLAUDE.md) and [spec/INDEX.md](spec/INDEX.md).

## Users

Primary users: broader, non-technical, packaged-app users — not limited to developers/self-hosters comfortable building from source. This means installers, onboarding, and UI polish matter more than a dev-focused open-source project would otherwise need. Concrete jobs: running many idle-game accounts open at once (background-throttled), managing multiple accounts across sites, and kiosk/embedded deployments — all needing many isolated browser sessions running light at once.

## Product Purpose

A browser built from scratch around running many isolated profiles side by side — light enough that dozens open at once cost a fraction of what Chromium-based multi-profile tools cost. Success means a non-technical user can run far more simultaneous isolated profiles than a Chromium-based competitor, on the same hardware, without the tool becoming the bottleneck.

## Positioning

The differentiator is **many isolated profiles running light at once** — per-profile OS-process isolation with far less memory/CPU overhead per profile than any Chromium-based multi-account/antidetect browser, achieved by building the rendering/JS engine from scratch (QuickJS, no JIT/huge heap) instead of embedding Chromium/CEF/Ultralight. Idle-game account management is one real use case among several (multi-account management, kiosk/embedded deployments) that all need this same property.

## Operating Context

Workflows: tiled panes of isolated profile sessions, workspaces (create/switch/move, background-throttled hidden panes), per-profile proxy/DNS/GPU-adapter selection, credential vault (OS keychain-backed), Chrome import (bookmarks/history/cookies/passwords), automation scripting per profile, resource monitoring (per-process CPU/RAM), downloads/history panels, i18n (EN/PT).

## Capabilities and Constraints

- **Target content is deliberately narrow**: controlled/curated sites this product is built for (idle games, dashboards, kiosk-style content) — not general web browsing. Full WPT/general-web conformance is explicitly not a goal (decided 2026-08-26, see [spec/ROADMAP.md](spec/ROADMAP.md) "Product scope decision").
- **Honest performance claim**: idle memory/CPU footprint and cold-start time at scale (many profiles) — not raw JS execution throughput or layout/paint throughput, where Chromium's V8 JIT and Skia+GPU compositor realistically win. Do not position messaging around beating Chromium on general browsing speed.
- No dev tools/inspector panel — deliberate non-goal, no devtools protocol implemented or planned.
- Windows is the primary supported OS for security features (Credential Manager, process sandboxing via Job Objects); Linux/macOS keychain and sandboxing are documented `Unsupported`.
- Naming is unresolved in the repo (README: idleGo, spec: IdleBrowser, mockup branding: Nimble) — **this PRODUCT.md and future work should use "Nimble"** (confirmed 2026-09-12), but README.md/spec docs still say otherwise and are not yet updated.

## Brand Commitments

Name: **Nimble** (confirmed 2026-09-12 as the name to standardize on going forward; not yet propagated to README.md or spec/ docs). Product mockup (`mockup/Nimble Browser.dc.html`) shows version string "Nimble 1.2.0" and an accent-colored wordmark — no logo asset beyond the text wordmark in the mockup.

## Evidence on Hand

No real user testimonials, case studies, press, or pricing exist — do not fabricate any. The only visual reference is the interactive HTML mockup at `mockup/Nimble Browser.dc.html` (UI/UX prototype, drives product features not implementation) and `mockup/browser-idle-spec.md` (technical execution spec).

## Product Principles

1. Many isolated, light profiles at once is the product — every feature decision should protect per-profile footprint over adding weight.
2. Target curated/controlled content, not general-web conformance — don't chase WPT completeness for its own sake.
3. Be honest about where this can and cannot compete with Chromium: idle footprint and cold-start, yes; raw JS/layout throughput, no.
4. Now aimed at non-technical, packaged-app users — onboarding, installer experience, and UI clarity matter, not just engine correctness.
5. No devtools/inspector — this is a deliberate scope cut, not a gap to fill.

## Accessibility & Inclusion

No product-specific accessibility requirement has been established yet.
