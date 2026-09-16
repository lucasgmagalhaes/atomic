# cef-shell

Thin Rust bindings over CEF's C API, via the [`cef`](https://github.com/tauri-apps/cef-rs) crate — see `spec/architecture/cef-integration.md` for the process-model/OSR design this crate implements against.

## Version pin

`cef = "152"` (CEF 152.3.0+152.0.6 at the time this crate was created). `cef-dll-sys`'s build script fetches the matching CEF binary distribution automatically on first build (~890MB, cached under `target/*/build/cef-dll-sys-*`) — no manual download/vendoring step. Bump the version deliberately, not automatically; a CEF/Chromium version bump changes real browser behavior (rendering, JS engine, security patches).

## Known gotcha: don't enable the default `sandbox` feature

`cef`'s default features include `sandbox`, which crashed (process `abort()`, exit code 3, no panic message) on the very first CEF API call (`Args::as_cmd_line`'s `command_line_create()`) when tested against this project's Windows setup. `cef-rs`'s own workspace `Cargo.toml` disables default features entirely and has its examples opt back into only what they need (e.g. `accelerated_osr`) — follow that convention: `cef = { version = "152", default-features = false, features = [...] }`, add features deliberately, and re-verify `cef_smoke` still runs after adding `sandbox` back if this project ever needs real OS-level sandboxing of the CEF subprocess tree (see `spec/ROADMAP.md` P4's sandbox-parity item).

## `cef_smoke` — the end-to-end proof binary

`cargo run -p cef-shell --bin cef_smoke [URL]` (defaults to `https://example.com`). Headless (no window — the real integration target is a raw pixel buffer for `crates/ipc`'s shared-memory transport, not an on-screen surface): initializes CEF, opens one off-screen browser against its own per-profile `RequestContext` (a temp `cache_path`, the real per-profile isolation knob), navigates, captures the first non-blank `on_paint` frame, and dumps it as a PPM to the OS temp dir plus prints its path. This is the thing to re-run after any change to this crate's CEF initialization sequence — it's cheap, real, and catches the exact class of "silent abort with no error message" bug this crate hit once already (see the file's own doc comment for the fix and why the fix was non-obvious).

Known open issue (not yet fixed, see `spec/ROADMAP.md` P0): CEF logs `Cannot create profile at path <cache_path>` for this binary's `RequestContext` regardless of whether the directory is pre-created, though the browser still renders afterward. Not yet confirmed whether the fallback still honors per-profile isolation or silently shares a default context — needs investigation before P1's isolation work relies on this pattern.

## `cef_profile_worker` — the real `profile-worker` replacement

Same CLI contract and stdin/stdout command protocol as `crates/profile`'s existing `profile-worker` binary (see that crate's `navigation.rs`/`interaction.rs`/`scripting.rs`), so `profile::Profile::spawn_full` can point at this binary instead without any change on the caller side. `PING`/`NAVIGATE`/`RELOAD` use CEF's real load pipeline; `EVAL`/`CLICK`/`FILL` go through the Chrome DevTools Protocol's `Runtime.evaluate` (`CefBrowserHost::execute_dev_tools_method`) — real return values, real thrown-exception messages with real JS stack traces, real full CSS selectors (`document.querySelector`, not the old engine's `#id`-only limitation). `CONSOLE` subscribes to CDP's `Runtime.consoleAPICalled` event.

Verified against a real page (`NAVIGATE https://example.com` → `EVAL document.title` → `"EVALUATED Example Domain"`; `CLICK` on a selector that doesn't exist → a real thrown `Error` with a real stack trace, not a made-up message).

**Real bug hit and fixed, worth knowing before touching this file**: `EVAL`/`CLICK`/`FILL`'s CDP result arrives via `on_dev_tools_method_result`, which — like `on_load_end`/`on_paint` — is only ever delivered *through* `do_message_loop_work()`. An earlier version of this file sent the CDP request and then blocked directly on the response channel (`rx.recv_timeout(...)`) without pumping the message loop in that same wait — every `EVAL`/`CLICK`/`FILL` call hung until its own timeout, 100% reproducible. The fix (`pump_until` in this file) is the same shape `NAVIGATE`'s own wait loop already used correctly from the start: loop calling `do_message_loop_work()` and polling the channel non-blockingly, not a single blocking `recv`.

`CLICK_AT`/`MOUSE_MOVE`/`SCROLL`/`KEY` (2026-09-16) are real coordinate/keyboard input injection via `CefBrowserHost::send_mouse_click_event`/`send_mouse_move_event`/`send_mouse_wheel_event`/`send_key_event` — genuine synthetic input events Blink's own pipeline processes, verified against a real focused `<input>` and a real `input` event listener, driven through `crates/profile::Profile`'s existing client methods unmodified. `KEY` is scoped to `"Backspace"` or one printable character, matching the old engine's own contract.

**Scope cut, honest not silent**: `DRAG_START`/`DROP_AT`/`CONTEXT_MENU_AT`/`COMPOSITION_*`/`TAB`/`TAB_REVERSE`/`RESIZE`/`SET_FPS_CAP`/`PAUSE`/`RESUME`, and the `proxy`/`dns_server`/`gpu_adapter` CLI args, aren't implemented yet — each replies `ERROR not yet implemented in the CEF-backed worker` (commands) or logs a visible warning (CLI args) rather than silently no-opping. Tracked in `spec/ROADMAP.md` P1.

## `crates/atomic` is already switched over

`crates/atomic`'s `Profile` now spawns `cef_profile_worker` instead of the old engine's `profile-worker` (2026-09-16, `browser_view.rs::worker_binary_path`) — see `spec/ROADMAP.md` P1 for the two real Windows sandbox bugs that had to be fixed first (`ActiveProcessLimit`/`PROFILE_MEMORY_LIMIT_BYTES` in `crates/security`/`crates/profile`, both sized for the old single-process engine) and which of `crates/atomic`'s own tests are honestly `#[ignore]`d against real, specific behavior differences.

## Chrome UI via CEF (planned, not yet built)

See `spec/architecture/chrome-ui.md`: the app's own shell UI (toolbar/sidebar/settings/etc., currently `egui`) is planned to move to a dedicated CEF browser rendering a ported version of `mockup/Atomic Browser.dc.html`, composited natively alongside per-profile panes rather than embedding them in the chrome page. The input injection above was built first specifically because the chrome UI needs the same real coordinate input a toolbar click requires.
