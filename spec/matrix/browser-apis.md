# Browser & Web API Classes — Capability Status

## 6. Browser and Web API classes

| API family | Done | Still needed |
| --- | --- | --- |
| Network | `fetch`, basic `XMLHttpRequest`, proxy/DNS plumbing | `Request`, `Response`, `Headers`, streaming bodies, abort, CORS, cache, redirects/cookies policy, WebSocket, EventSource |
| Timers | `setTimeout`, `setInterval`, `requestAnimationFrame`, cancellation | microtask/checkpoint semantics, idle callbacks, visibility/background throttling policy |
| URL | host URL handling in navigation/resources | `URL`, `URLSearchParams`, origin model, base URL and full relative-resolution exposure |
| Storage | `localStorage`, `sessionStorage`, cookies, IndexedDB primitives | quotas, transactions/indexes/cursors, storage events, Cache Storage, service workers |
| Files | `Blob`, `File`, object URLs | `FileReader`, drag/drop files, streams, filesystem access policy |
| Media | Web Audio offline graph primitives | `AudioContext`, media elements, `MediaSource`, WebRTC, permissions/device selection |
| Device/UI | clipboard, notifications, performance | geolocation, permissions, screen, history, location, fullscreen, dialogs, popups |

---

[← back to spec/INDEX.md](../INDEX.md)
