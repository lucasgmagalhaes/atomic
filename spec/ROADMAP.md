# Roadmap — CEF Pivot

Phased plan for replacing the `neutron` custom engine with a CEF-backed browser shell. Each phase should land as its own set of commits and keep `cargo build --workspace` green (once the workspace itself is updated to drop the deleted crates — see P0).

## P0 — Remove the old engine, land the CEF skeleton

- `[x]` Delete the *dead* part of the old engine (2026-09-16): `crates/atomicjs` (zero external users) and `crates/atomic`'s "Track B" chrome-engine spike (`chrome_engine/*`, `chrome_bridge/*`, `chrome_*_ui.rs` — an additive, never-live proof-of-concept, not the real UI). `crates/neutron` itself is **not** deleted yet — see the next item, this was a real dependency ordering discovery, not an oversight.
- `[x]` Migrate `crates/automation` off `neutron::js`/raw `quickjs-sys` onto the standalone `rquickjs` crate (2026-09-16) — this was the actual remaining blocker for deleting `neutron`'s JS engine: automation's `pane`/`cron`/`on`/`every` scripting DSL needs its own embedded JS interpreter independent of page rendering, and it only used `neutron::js` because both used to be the same engine. `[ ]` Deleting `crates/neutron/*` (all 11 sub-crates + facade) itself is still open — blocked on P1's profile-worker replacement (the one real remaining consumer) and the GPU-adapter-picker item below.
- `[ ]` `crates/atomic/src/app/settings_ui.rs`'s GPU adapter dropdown (`neutron::paint::list_adapters()`) is the one other real `neutron` dependency left in `crates/atomic` — needs a CEF-era equivalent (or a documented "not exposed as a setting anymore" decision) before `neutron` can be deleted.
- `[ ]` Remove the `xtask check-neutron-boundary` command and its pre-commit hook wiring once `crates/neutron` itself is actually deleted (`spec-archive-neutron-engine/proposals/NEUTRON_ENCAPSULATION.md`'s boundary no longer applies then — no sub-crates to bound). Keep it until then, it's still valid.
- `[x]` Add a new `crates/cef` crate (package `cef-shell`, 2026-09-16): depends on `cef` (the `tauri-apps/cef-rs` project's safe wrapper over CEF's C API), **with `default-features = false`** — the default `sandbox` feature crashed (abort, exit code 3) on the very first CEF API call in testing; upstream's own workspace disables it too and its examples opt back into only what they need. `cef-dll-sys`'s build script fetches the CEF binary distribution automatically on first build (~890MB, cached in the target dir) — no manual vendoring step needed, contrary to this item's original phrasing.
- `[x]` One CEF-backed "window" rendering a real page end to end (2026-09-16, `crates/cef/src/bin/cef_smoke.rs`): headless off-screen rendering (no window at all, since the real target is a raw pixel buffer for `crates/ipc`, not an on-screen surface) — real multi-process CEF (GPU/utility/network/storage/renderer processes all really spawned, confirmed in the smoke test's own log), a real per-profile `RequestContext` with its own `cache_path`, a real navigation to `https://example.com`, and a real non-blank `on_paint` BGRA frame captured and dumped to a PPM — visually confirmed as the actual page (title "Example Domain" and its body text), not a placeholder. Known open issue logged during this: CEF's own `chrome_browser_context.cc` logs "Cannot create profile at path" for the `cache_path` directory regardless of whether it's pre-created, yet the browser still renders afterward — **not yet confirmed** whether the real per-profile isolation the `cache_path` was meant to give is actually in effect or whether it silently fell back to a shared context. Investigate before relying on this for P1's isolation guarantees.

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
