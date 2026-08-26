# Navigation, Security & Lifecycle — Capability Status

## 7. Navigation, security and lifecycle

> **Reconciled 2026-08-26** against `spec/matrix/changelog.md` — the original Done/Needed split here predated several changelog items and was stale (claiming `window`/`location`/CORS/CSP/lifecycle as "Needed" when the changelog already showed them done). Percentage not recomputed; treat as "mostly done, see Needed below for what's real."

**Done**

- [x] Navigate, reload, error-page fallback and per-host storage roots.
- [x] Resource fetching for HTML, CSS and images; profile worker runs document scripts in source order.
- [x] Bounded DOM/event inputs and safe typed host bindings.
- [x] `window`/`globalThis`/`self`/`top`/`parent` aliases, `location` (read-only), `history` (`pushState`/`replaceState`/`back`/`forward`/`go`), `navigator`, `screen` — see changelog item 5 and `crates/js-runtime/src/navigator_screen*` (git log `feat(navigator/screen)`).
- [x] Lifecycle: `DOMContentLoaded`, `load`, cancelable `beforeunload` — see changelog item 5.
- [x] Same-origin policy, opaque origins, CORS (simple-request subset), mixed-content blocking, referrer policy (`strict-origin-when-cross-origin`) — see changelog item 7 (`crates/js-runtime/src/cors.rs`).
- [x] CSP enforcement (`connect-src`/`default-src`, header + `<meta http-equiv>` delivery) and Trusted Types (`require-trusted-types-for 'script'` gating `innerHTML`/`outerHTML`/`insertAdjacentHTML`) — see changelog item 7 (`crates/js-runtime/src/csp.rs`, `trusted_types.rs`).
- [x] Permissions Policy for the capability surfaces Nimble exposes (`clipboard-read`/`clipboard-write`, `notifications`) — `Context::set_permissions_policy`.

**Needed**

- [ ] `location`/`history` write-side navigation (`location.href = ...`, `assign`/`replace`/`reload`) — no channel yet for JS to ask its host to actually navigate.
- [ ] Viewport APIs (`visualViewport`, resize-driven re-layout wiring).
- [ ] `pagehide`, visibility/focus lifecycle beyond the `page_visibility` static placeholder, bfcache policy.
- [ ] Sandboxed documents, multiple browsing contexts: no `iframe`/frame tree exists anywhere in `dom` — popup policy and cross-window messaging are structurally blocked on this.
- [ ] Request cancellation (`AbortController`/`AbortSignal`), navigation races, atomic document replacement.
- [ ] Secure cookie attribute completeness beyond `SameSite` (already done, see `storage::cookies`) — no public-suffix-list-aware domain checks.

---

[← back to spec/INDEX.md](../INDEX.md)
