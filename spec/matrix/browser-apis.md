# Browser & Web API Classes — Capability Status

## 6. Browser and Web API classes

Stale as of 2026-09-14: this table predates most of this engine's real browser-API work and was still describing a much earlier state (e.g. listing `Request`/`Response`/`Headers`/abort/CORS/`URL`/`URLSearchParams`/screen/history/location/notifications/clipboard/performance as "still needed" when every one of them is real, `crates/js-runtime/src/`-grep-confirmed). Rewritten below against what's actually in the tree today.

| API family | Done | Still needed |
| --- | --- | --- |
| Network | `fetch`/`fetchSync`, `XMLHttpRequest`, real `Request`/`Response`/`Headers` (`request_response/`), `AbortController`/`AbortSignal` (`abort_controller/`), same-origin/CORS + mixed-content enforcement (`cors.rs`), CSP `connect-src`/`script-src` (`csp.rs`), referrer policy, proxy/DNS plumbing | Streaming request/response bodies (bodies are whole-buffer), a real HTTP cache (`network::ResourceCache` is per-page-load only, no cross-navigation freshness/expiry), redirect following, `WebSocket`, `EventSource` |
| Timers | `setTimeout`/`setInterval`/cancellation, `requestAnimationFrame` (`timers.rs`), real interrupt/time-budget execution limits (`script_limits.rs`) | `requestIdleCallback`, visibility/background throttling policy (`page_visibility.rs`'s `document.hidden` is a real, always-`"visible"` placeholder — nothing throttles a hidden page's timers differently) |
| URL | Real `URL`/`URLSearchParams` (`url_bindings/`: constructor, all parts, `searchParams`, iterators), real origin model (`cors::page_origin`, opaque-origin handling for `data:`/`blob:`), base-URL relative resolution (`url::Url::join`, used by module resolution/import maps/`resolve_url`) | Nothing significant identified — this row can likely be dropped/merged into Network in a future pass |
| Storage | `localStorage`/`sessionStorage` (`local_storage_bindings.rs`), real cookie jar (`Set-Cookie` parsing, `SameSite`, RFC 6265 domain/path matching), IndexedDB (`indexed_db_bindings.rs`: object stores, one secondary index, cursors — scoped down, see `CLAUDE.md`) | Storage quotas, IndexedDB versioned schema/async transactions, `StorageEvent` (a same-process `localStorage` write never notifies another window), Cache Storage (`caches`), Service Workers |
| Files | `Blob`/`File` (`blob/`), object URLs (`URL.createObjectURL`/`revokeObjectURL`), `FormData` with Blob/File entries (`form_data/`) | `FileReader`, drag/drop file input, streams (`Blob.stream()`), filesystem access policy |
| Media | Web Audio offline graph primitives (`web_audio/`: `AudioContext`-shaped graph, nodes, buffers) | Real-time `AudioContext` output (no actual audio device output exists), `<audio>`/`<video>` media elements, `MediaSource`, WebRTC, media device permissions/selection |
| Device/UI | Clipboard (`clipboard.rs`), Notifications (`notifications.rs`), Performance (`performance.rs`), Screen (`screen.rs`), History (`history/`: `pushState`/`replaceState`/`back`/`forward`/`go`), Location (`location.rs`), real per-window multi-context support + `postMessage` (`window_registry.rs`), Selection/Range (`selection.rs`) | Geolocation, a real `navigator.permissions` Permissions API (`permissions_policy.rs` is the unrelated CSP-adjacent Permissions-Policy header gate, not this), Fullscreen API, `alert`/`confirm`/`prompt` dialogs, popup-blocking policy |

---

[← back to spec/INDEX.md](../INDEX.md)
