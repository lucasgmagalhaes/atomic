# CEF Integration — Process Model & Rust Boundary

## Why CEF, not a raw Chromium checkout

Chromium itself is a ~40-hour build with no stable embedding API. CEF (Chromium Embedded Framework) is the standard third-party layer that packages Chromium as an embeddable library with a stable, documented C/C++ API and prebuilt binary distributions per platform/version — used by Steam, many Electron-alternative desktop apps, and most CEF-based multi-account browsers already in this market. Building directly against Chromium's internal APIs would mean tracking an unstable internal surface; CEF trades a small amount of control for a maintained, versioned embedding contract.

## Process model

CEF is multi-process by default, same as stock Chrome:

- **Browser process** — one per application instance. Owns the CEF message loop, window management, and (in this project) the `crates/atomic` GUI shell.
- **Renderer process(es)** — one per renderer/site-instance, runs Blink + V8, sandboxed.
- **GPU process** — shared, handles compositing/rasterization.
- **Utility processes** — network service, audio, etc., same as stock Chrome.

The open design question this pivot must answer (`spec/ROADMAP.md` P1): does "one profile = one isolated `CefRequestContext`" inside a single CEF browser-process instance give strong enough isolation, or does this product's isolation bar require spawning a **separate CEF browser-process instance per profile** (heavier, but a harder isolation boundary — a full separate Chromium instance, not just a separate context within one). Stock multi-account-browser products in this space (GoLogin, Multilogin, AdsPower) use the separate-instance model specifically because context-level isolation inside one instance has had real historical leak bugs (some cache/GPU-shader/font-cache state has at times been process-global, not per-context). Default to **separate CEF instances per profile** unless a specific perf/startup number makes that untenable — isolation is the product; don't compromise it for a startup-time win without measuring first.

## Off-screen rendering (OSR)

`crates/atomic`'s GUI shell composites its own panes (workspaces, grid layout, multiple profile views in one window) rather than each profile owning a native OS window. This means CEF must run in **OSR mode** (`CefWindowInfo::SetAsWindowless`), which delivers rendered frames via the `CefRenderHandler::OnPaint` callback as a raw BGRA buffer instead of drawing to a native window directly.

- `OnPaint`'s buffer feeds into `crates/ipc`'s existing shared-memory double-buffer transport (this crate is engine-agnostic and doesn't need to change — it never assumed `neutron::render`'s output format specifically, just "a frame buffer").
- Input (click/scroll/key/fill) goes the other direction via `CefBrowserHost::SendMouseClickEvent`/`SendKeyEvent`/etc. — replaces the old `CLICK`/`FILL` stdin-command protocol to `profile-worker`.
- OSR has real, documented downsides vs. windowed mode: no native GPU compositing path in some CEF versions (falls back to software or a shared offscreen GL context), and some accessibility/IME features are weaker. Verify these against this project's actual needs before committing further — flag to the user if OSR turns out to cost more perf than a native-window-per-profile model would.

## Rust ↔ CEF C API boundary

CEF's public API is C++, but it also ships a **C API** (`include/capi/`) specifically for FFI from other languages — this is the intended integration point, not hand-binding the C++ vtables.

- `crates/cef`: raw `bindgen`-generated (or `cef-sys` if that crate proves current and correctly versioned) bindings to the CEF C API, plus minimal safe Rust wrappers (RAII for `cef_base_ref_counted_t`-based refcounting, safe callback trampolines for `CefClient`/`CefRenderHandler`/etc. C function-pointer structs).
- No product logic in `crates/cef` — it should be crate-agnostic enough that `crates/atomic` and any future profile-management crate both depend on it, not the other way around (see `spec/RULES.md`'s SRP rule).
- Follow the same category of gotchas the old `js-runtime` had with QuickJS's C API (see `CLAUDE.md`'s Known Gotchas) — expect CEF's refcounted C API to have similar sharp edges (owned vs. borrowed reference returns, callback lifetime rules). Document new gotchas here as they're discovered, the same way the old engine's quickjs gotchas were documented.

---

[← back to spec/INDEX.md](../INDEX.md)
