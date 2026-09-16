# Isolation & Performance — The Actual Product

Chromium already wins on rendering/JS correctness and compatibility outright — that stopped being a place to compete the moment this project adopted it. Everything below is where this product still has to build something real, because stock Chromium-based apps (including Chrome itself) aren't optimized for "many isolated, mostly-idle profiles at once."

## Default isolation between profiles

"Default" means a new profile is isolated unless something explicitly shares state — never the reverse.

Scope of isolation, all per-profile via a dedicated `CefRequestContext` (or dedicated CEF instance — see `cef-integration.md`'s open question):
- Cookies, `localStorage`/`sessionStorage`, IndexedDB, Cache API, Service Worker registrations.
- HTTP cache and cache-based timing side channels.
- Proxy configuration and DNS resolution.
- Permissions (camera/mic/geolocation/notifications) — a grant in one profile must never apply to another.
- TLS session resumption cache (BoringSSL) — check whether CEF scopes this per-context; if it's process-global, two profiles sharing a process could be correlated by a passive network observer even with everything else isolated.
- GPU shader cache / font cache — historically some Chromium-derived products have leaked these across contexts; verify, don't assume.

Explicitly **not** in scope: fingerprint spoofing (UA string, canvas/WebGL noise, timezone/locale randomization). Carried over unchanged from the old engine's product-scope decision — this product's isolation story is about not leaking real user data/sessions across profiles a person legitimately manages, not about evading platform detection. Revisit only with the user, explicitly (see `spec/ROADMAP.md` P4's last item).

## Startup performance

Two distinct numbers matter and should be tracked separately:

1. **Cold app start** — process launch to first profile's first paint. Dominated by CEF/Chromium's own init cost (sandbox setup, GPU process spawn, network service spawn). Budget target: sub-1s on typical hardware.
2. **Marginal profile open** — time to open the *second* (and Nth) profile once the app is already running. This is the number that actually matters for the "many profiles" use case and should be much cheaper than cold start if subprocess pre-warming (`spec/ROADMAP.md` P2) works. Budget target: sub-300ms.

Levers, roughly in order of expected impact:
- Command-line switch audit to disable unused Chromium subsystems (translate, spellcheck service, component updater/auto-update pings, crash reporting, background sync) — each is both a startup-time and idle-resource cost.
- Renderer process pre-warming/pooling.
- Lazy-init anything in `crates/atomic`'s own startup path not needed for first paint (settings UI, automation engine, i18n bundles, GPU adapter enumeration for the settings dropdown).
- OSR-specific: confirm CEF's OSR init path isn't paying for a full native-window compositor setup it then throws away.

## Idle footprint at scale

The actual differentiator claim ("more isolated profiles per GB of RAM than a stock multi-account Chromium tool") needs a concrete, defended number, not a vibe. Two levers:

- **Per-profile idle memory** — a backgrounded profile should be discardable: renderer process evicted, only `CefRequestContext`'s on-disk state kept (cookies/storage survive on disk, nothing resident in RAM), restored on next foreground. This is the single biggest lever — Chrome itself does this for background tabs (tab discarding) but is conservative about *when*; a product built around "profiles that are open all day but rarely looked at" can be much more aggressive.
- **Per-profile idle CPU** — background renderer throttling (timer/requestAnimationFrame throttling, no compositing/painting for non-visible surfaces). Mostly a stock Chromium behavior already; verify it isn't accidentally disabled by running in OSR/windowless mode (some throttling heuristics key off native window occlusion, which windowless mode doesn't have — may need to explicitly signal visibility state via `CefBrowserHost::WasHidden` instead of relying on the default heuristic).

Measure with `platform-apis`' existing per-process CPU/RAM sampling (kept from the old architecture — it's engine-agnostic) against CEF's actual renderer/GPU processes, not against the old `profile-worker` processes it used to sample.

---

[← back to spec/INDEX.md](../INDEX.md)
