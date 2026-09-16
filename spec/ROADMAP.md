# Roadmap — CEF Pivot

Phased plan for replacing the `neutron` custom engine with a CEF-backed browser shell. Each phase should land as its own set of commits and keep `cargo build --workspace` green (once the workspace itself is updated to drop the deleted crates — see P0).

## P0 — Remove the old engine, land the CEF skeleton

- `[ ]` Delete `crates/neutron/*` (all 11 sub-crates + facade) and `crates/atomicjs`. Remove them from the workspace `Cargo.toml` `members` list.
- `[ ]` Remove every direct/facade reference to `neutron`/`atomicjs` from `crates/atomic` (currently 16 files under `crates/atomic/src/` reference `neutron` — `chrome_engine/*`, `chrome_bridge/*`, `automation_bridge.rs`, `browser_view.rs`, `app/*`, `settings.rs`). These become CEF-backed instead (see P1).
- `[ ]` Remove the `xtask check-neutron-boundary` command and its pre-commit hook wiring (`spec-archive-neutron-engine/proposals/NEUTRON_ENCAPSULATION.md`'s boundary no longer applies — no sub-crates to bound).
- `[ ]` Add a new `crates/cef` crate: thin Rust bindings over CEF's C API (via `cef-sys`/hand-rolled bindgen — evaluate `cef-rs` first, fall back to a minimal hand-written binding scoped to what this project needs if `cef-rs` is unmaintained or too heavy).
- `[ ]` Vendor/fetch the CEF binary distribution (cef_binary from the CEF automated builds) per-platform; document the pinned CEF/Chromium version in `crates/cef/README.md`.
- `[ ]` One CEF-backed window rendering a real page end to end (off-screen rendering / OSR mode, since `crates/atomic`'s GUI shell composites panes itself — see `architecture/cef-integration.md`).

## P1 — Replace `profile`/`profile-worker` with a CEF-backed profile process

- `[ ]` Each profile still gets its own OS process (CEF's own multi-process model: browser process + renderer/GPU processes per CEF context) — decide whether one CEF "request context" per profile inside a shared browser process is enough, or whether this product's isolation bar requires a separate CEF sub-process tree per profile (see `architecture/isolation-and-perf.md` — this is the central design decision of the pivot, don't skip it).
- `[ ]` Cookie/localStorage/IndexedDB/cache isolation per profile via CEF's `CefRequestContext` (one per profile, own cache path, own cookie manager) — replaces `storage` crate's hand-rolled localStorage/cookie-jar/IndexedDB.
- `[ ]` Proxy/DNS per profile via `CefRequestContext`'s proxy resolution — replaces `net` crate's per-request proxy/DNS plumbing.
- `[ ]` Navigation/reload/click/fill/eval command surface equivalent to the old `NAVIGATE`/`RELOAD`/`PING`/`QUIT`/`CLICK`/`FILL`/`EVAL` stdin protocol, now via CEF's `CefBrowser`/`CefFrame` APIs (`LoadURL`, `ExecuteJavaScript`, input events) instead of hand-rolled IPC to a home-grown worker.
- `[ ]` OSR frame delivery into `crates/ipc`'s existing shared-memory double-buffer transport (keep `ipc` — it's engine-agnostic, just needs to accept CEF's `OnPaint` buffer instead of `neutron::render`'s output).

## P2 — Startup performance

- `[ ]` Measure current CEF cold-start baseline (process spawn → first paint) per profile; set a concrete budget (e.g. sub-300ms per additional profile after the first, sub-1s for the first).
- `[ ]` Shared CEF subprocess pre-warming: keep a small pool of pre-forked renderer processes ready so opening a new profile doesn't pay full process-spawn + CEF-init cost.
- `[ ]` Defer/lazy-init anything not needed for first paint (GPU adapter selection, automation engine bindings, i18n bundle loading) — audit `crates/atomic/src/app/mod.rs`'s startup path.
- `[ ]` `--disable-*` CEF/Chromium command-line switch audit: strip background services this product doesn't need (translate, spellcheck network features, component updater, crash reporting unless opted in) — each one is both an idle-CPU/memory cost and a startup-time cost.

## P3 — Idle footprint & tab/profile throttling

- `[ ]` Background-profile throttling: suspend rendering (already done at the `crates/atomic` GUI level for hidden panes) plus request Chromium's own background tab throttling (`--enable-background-tab-throttling` equivalent behavior is default in stock Chromium — verify it survives OSR mode, which sometimes disables the normal occlusion-based throttling heuristics).
- `[ ]` Per-profile idle memory measurement (reuse `platform-apis`' existing per-process CPU/RAM sampling, now sampling CEF renderer processes) — set a target and regression-test against it.
- `[ ]` Discard/reload strategy for long-idle background profiles (evict renderer, keep cookies/storage via `CefRequestContext`'s on-disk cache, restore on next foreground) — the actual mechanism that lets "many profiles open at once" stay cheap.
- `[ ]` GPU process sharing vs per-profile GPU process: evaluate whether one shared GPU process (stock Chromium default) is fine, or whether isolation requirements push toward `--disable-gpu` + software compositing for background profiles specifically.

## P4 — Default isolation hardening

- `[ ]` Verify no cross-profile leakage: cookies, localStorage, IndexedDB, HTTP cache, service workers, permissions (camera/mic/notifications), and the CEF equivalent of Chrome's "Clear-Site-Data" all scoped strictly per `CefRequestContext`. Write an integration test that opens two profiles and asserts zero shared state.
- `[ ]` Per-profile TLS session cache isolation — check whether CEF/BoringSSL's TLS session resumption cache is scoped per `CefRequestContext` or process-global (a subtle cross-profile fingerprint/correlation leak if global — same "no fingerprint spoofing but no accidental correlation either" logic as before).
- `[ ]` Sandbox parity: keep the existing Windows Job Objects sandboxing (`security` crate) around the CEF renderer processes, or confirm CEF's own sandbox (already Job-Objects-based on Windows) supersedes it and remove the now-redundant custom code.
- `[ ]` Re-verify the "no fingerprint spoofing" decision (`spec-archive-neutron-engine/ROADMAP.md`'s Product scope decision) still holds under CEF — Chromium's real UA/canvas/WebGL fingerprint surface is much bigger than the old engine's, so "we don't spoof it" is a materially different (larger) attack surface than before. Flag to the user before shipping, don't assume silently.

## Product scope carried over from the old plan

The multi-profile / low-footprint positioning itself (`spec-archive-neutron-engine/ROADMAP.md`'s "Product scope decision", 2026-08-26) still holds and is *more* achievable now — Chromium gives full site compatibility for free, so the "controlled/curated content only" scope limitation from the old engine no longer applies. The differentiator narrows to exactly what P2–P4 above build: startup time, idle footprint, and isolation, not rendering completeness (Chromium already wins that outright). Don't reposition marketing around raw rendering speed — with real Chromium underneath, that's no longer a differentiator at all (everyone has the same engine); the pitch is entirely about the many-isolated-profiles-at-once economics.

---

[← back to spec/INDEX.md](INDEX.md)
