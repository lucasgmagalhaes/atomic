# Chrome UI via CEF — Design

Decided 2026-09-16, following the "how does Zen Browser do it" discussion: the app's own shell UI (toolbar, workspace sidebar, tab strip, Monitor/Automation/Downloads/Settings screens, modals — everything in `mockup/Atomic Browser.dc.html` except the actual page content inside a pane) moves from hand-drawn `egui` to a real HTML/CSS/JS page (React or plain JS — implementation detail, not an architectural one), rendered by its own dedicated CEF browser instance. Same reasoning Zen Browser (a Firefox fork) uses for its own customized UI: Firefox's chrome has always been a privileged HTML/XUL document rendered by the *same* Gecko engine already running for content, not a second engine spun up just for the UI. This project's equivalent: **one more CEF browser, not a second engine** — `crates/cef` already exists, this reuses it.

## Why this over `egui`

- The mockup is already authored in HTML/CSS with real interactivity bindings (`onClick`, `sc-for`, `sc-if`) — porting it to Rust/`egui` by hand means re-deriving every layout/style decision a second time in a completely different toolkit, and re-doing it again on every mockup revision.
- CSS gives real, cheap access to the mockup's actual visual language (gradients, box-shadows, the exact spacing/typography system in `mockup/_ds/`) that `egui`'s immediate-mode styling API doesn't have equivalents for.
- The dedicated CEF chrome browser is one browser process for the whole app's lifetime, not per-profile — it does **not** multiply with profile count the way content workers do, so it doesn't work against the pivot's per-profile-footprint goal. Budget it once, like Zen budgets one Gecko chrome document once.

## Why not render pane content *inside* the chrome page

A tempting simplification: if the chrome is already a web page, why not make each pane an `<iframe>` or a `<canvas>` fed by the profile's frame bytes, all inside one page? Rejected:

- **Isolation**: an `<iframe>` embedding another origin's content inside the chrome's own page defeats the whole point of per-profile process isolation this pivot is built around — a compromised or hung page could reach into the chrome document's own privileged JS context in ways a fully separate OS process cannot.
- **Cost**: feeding a profile's OSR frame into a `<canvas>` means copying it a second time (shared memory → bridge → `ImageData` → GPU upload inside the chrome browser's own compositor) on top of the copy `crates/ipc`'s `FrameWriter`/`FrameReader` already does — real, avoidable per-frame cost at exactly the scale (many profiles, every frame) this project cares most about.

Instead: **native compositing, unchanged from today.** `crates/atomic`'s own render loop already reads a profile's OSR buffer via `ipc::FrameReader` and uploads it as a GPU texture at the pane's grid rect (see `browser_view.rs`); that pipeline doesn't change. What changes is *only* the layer underneath and around the panes: instead of `egui` drawing the toolbar/sidebar/etc., the chrome CEF browser's own OSR buffer is uploaded as a full-window background texture, painted first, with each pane's live texture painted on top at its grid rect — the chrome page's pane-grid area is just empty/transparent space the native code knows to leave alone and draw over. One more texture upload per frame, not a second render engine for content.

## Two kinds of CEF browser, not one

| | Chrome browser | Content worker (`cef_profile_worker`) |
|---|---|---|
| Count | 1 per app instance | 1 per profile |
| Process | Its own OS process (same isolation posture as content — a chrome crash shouldn't be a special case) | Its own OS process (already built) |
| `RequestContext` | Default/shared — no per-profile isolation concern, it never loads untrusted content | Own `cache_path` per profile (already built, P0/P1) |
| Loads | A local `data:`/`file://` HTML bundle (the ported mockup), never a remote URL | Whatever URL the profile navigates to |
| Lifetime | App startup → app exit | Profile spawn → profile close |

## The bridge: chrome page ↔ Rust host

Two directions, two different real mechanisms — deliberately not the same one:

**Rust → JS (push state):** `CefBrowserHost::execute_dev_tools_method`'s `Runtime.evaluate`, already built and proven in `cef_profile_worker.rs`. The host calls something like `window.__atomicBridge.setState(<json>)` whenever app state changes (workspace switched, a pane's status changed, a download progressed); the page's own JS (a plain object, or React's `setState` underneath) re-renders from it. No new mechanism — this is the *exact* CDP call `EVAL` already makes.

**JS → Rust (actions):** the chrome page has no server to call and no direct FFI - CDP is host-initiated, not page-initiated, so `Runtime.evaluate` doesn't help this direction. Reuse `CONSOLE`'s already-built plumbing instead: `on_dev_tools_event`'s `Runtime.consoleAPICalled` subscription (already wired for `CONSOLE` in `cef_profile_worker.rs`) is a *real*, working page → host channel with zero extra CEF wiring needed. The chrome page calls `console.log(JSON.stringify({action: "switchWorkspace", id: "..."}))` for every user action; the host's dev tools observer parses a magic-prefixed console message and dispatches it as a real `AtomicApp` method call instead of appending it to a displayed log.

This is deliberately the *simplest* mechanism that works, not the most "proper" one (CEF's real answer to this is `cefQuery`/`CefMessageRouter`, which requires a `RenderProcessHandler` — a second, renderer-process-side piece of app logic this project doesn't have any other reason to build yet). Revisit if the console-log channel's throughput or message-size limits ever become real problems — CDP console messages aren't designed to be a high-frequency RPC transport, and large payloads may need chunking or a real `cefQuery` migration.

## Input routing (new real work, not yet built)

Today, `crates/atomic` reads pixels from panes but only sends *element-targeted* commands (`CLICK #selector`, `FILL`) via CDP, not raw coordinate input — real mouse/keyboard interaction with an actual page (scrolling, clicking a non-selector-addressable spot, typing into a focused field the natural way) isn't implemented yet (`spec/ROADMAP.md` P1's open `CLICK_AT`/`MOUSE_MOVE`/`KEY`/etc. list). The chrome UI needs exactly the same real input injection — clicking the mockup's own toolbar buttons is genuine page interaction, not something `Runtime.evaluate` should fake.

This pulls that work forward: `crates/atomic`'s input loop must hit-test a raw mouse/key event against screen regions (chrome vs. which pane's rect) and forward it to the right target's `CefBrowserHost` via CEF's real input APIs (`send_mouse_click_event`/`send_mouse_move_event`/`send_key_event`, all real methods already in the `cef` crate's bindings, just unused so far). One shared input-routing/injection layer serves both the chrome browser and every content worker — build it once, in `crates/cef`, not twice.

## Implementation order

1. `crates/cef`: real coordinate-based input injection (`send_mouse_click_event`/`send_mouse_move_event`/`send_key_event`) on `cef_profile_worker`'s existing protocol (new stdin commands, closing several of P1's still-open items) — needed by both content panes and the chrome browser, build it against the already-proven worker first where it's easiest to test in isolation.
2. A `cef_chrome` binary (or a mode of the same binary family) — same OSR/init pattern as `cef_profile_worker`, loading a local HTML bundle instead of a navigated URL, no per-profile `RequestContext` concerns.
3. Port `mockup/Atomic Browser.dc.html` to a real static bundle (React or plain JS - the mockup's own `{{ }}`/`sc-for`/`sc-if` syntax is this specific design tool's templating, not runnable as-is) wired to the `setState`/console-log bridge above with *static/fake* data first, proving the render pipeline end to end before wiring real app state.
4. `crates/atomic`: replace `egui`-drawn chrome panels with the chrome texture + native compositing described above, wire the bridge to real `AtomicApp` methods one screen at a time (toolbar first, since every other screen depends on tab/workspace switching already working).

Do not attempt to port the whole mockup in one pass — land the toolbar end-to-end (spawn chrome browser, real click reaching a real `switch_workspace` call, real state pushed back and reflected in the page) as the first working slice, matching how `cef_smoke`/`cef_profile_worker` were each proven standalone before the next piece was built on top.

---

[← back to spec/INDEX.md](../INDEX.md)
