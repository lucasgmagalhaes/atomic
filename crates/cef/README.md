# cef-shell

Thin Rust bindings over CEF's C API, via the [`cef`](https://github.com/tauri-apps/cef-rs) crate — see `spec/architecture/cef-integration.md` for the process-model/OSR design this crate implements against.

## Version pin

`cef = "152"` (CEF 152.3.0+152.0.6 at the time this crate was created). `cef-dll-sys`'s build script fetches the matching CEF binary distribution automatically on first build (~890MB, cached under `target/*/build/cef-dll-sys-*`) — no manual download/vendoring step. Bump the version deliberately, not automatically; a CEF/Chromium version bump changes real browser behavior (rendering, JS engine, security patches).

## Known gotcha: don't enable the default `sandbox` feature

`cef`'s default features include `sandbox`, which crashed (process `abort()`, exit code 3, no panic message) on the very first CEF API call (`Args::as_cmd_line`'s `command_line_create()`) when tested against this project's Windows setup. `cef-rs`'s own workspace `Cargo.toml` disables default features entirely and has its examples opt back into only what they need (e.g. `accelerated_osr`) — follow that convention: `cef = { version = "152", default-features = false, features = [...] }`, add features deliberately, and re-verify `cef_smoke` still runs after adding `sandbox` back if this project ever needs real OS-level sandboxing of the CEF subprocess tree (see `spec/ROADMAP.md` P4's sandbox-parity item).

## `cef_smoke` — the end-to-end proof binary

`cargo run -p cef-shell --bin cef_smoke [URL]` (defaults to `https://example.com`). Headless (no window — the real integration target is a raw pixel buffer for `crates/ipc`'s shared-memory transport, not an on-screen surface): initializes CEF, opens one off-screen browser against its own per-profile `RequestContext` (a temp `cache_path`, the real per-profile isolation knob), navigates, captures the first non-blank `on_paint` frame, and dumps it as a PPM to the OS temp dir plus prints its path. This is the thing to re-run after any change to this crate's CEF initialization sequence — it's cheap, real, and catches the exact class of "silent abort with no error message" bug this crate hit once already (see the file's own doc comment for the fix and why the fix was non-obvious).

Known open issue (not yet fixed, see `spec/ROADMAP.md` P0): CEF logs `Cannot create profile at path <cache_path>` for this binary's `RequestContext` regardless of whether the directory is pre-created, though the browser still renders afterward. Not yet confirmed whether the fallback still honors per-profile isolation or silently shares a default context — needs investigation before P1's isolation work relies on this pattern.
