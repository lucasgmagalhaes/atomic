# atomic

The browser's GUI shell — the `atomic` binary (`Atomic` in the title bar).
An `eframe`/`egui` desktop app: one `profile-worker` process per open pane
(tab), workspaces (create/switch/move, background-throttled hidden panes),
downloads/history panels, i18n (EN/PT), the add-profile modal, Chrome
import, and Settings (max panes, FPS cap, GPU adapter, OS-keychain
toggle). See `CLAUDE.md`'s "Implementation status" for the full feature
list and `mockup/Atomic Browser.dc.html` for the UI/UX source of truth.

## What it depends on

- `neutron` — the render/execute engine. `atomic` never path-deps on
  `neutron`'s sub-crates (`css`/`html`/`dom`/`layout-engine`/`render`/
  `js-runtime`/etc.) directly; see `crates/neutron/README.md`.
- `profile` / `automation` — spawns and drives the per-tab
  `profile-worker` processes and the automation-scripting engine.
- `net`, `storage`, `platform-apis`, `security`, `import` — platform
  services (networking, cookies/localStorage, CPU/RAM sampling +
  clipboard, credential vault, Chrome bookmark/history/cookie import).

## `chrome_engine`/`chrome_bridge` — a spike, not the main render path

One piece of the shell's own chrome (the toolbar, settings, downloads/
history panel, add-profile modal) is rendered by Atomic's own engine
(`neutron`) instead of `egui`, proving the same load/layout/render/
hit-test path a real page uses works for native UI chrome too. This is
deliberately self-contained (doesn't reuse `profile-worker`'s private
`Page` type) — see `chrome_engine/mod.rs`'s module doc.

## Why this crate lives at `crates/atomic` and not `apps/shell`

It used to be `apps/shell` (package name `shell`) — the only member of a
now-removed `apps/` directory, which existed for an `apps/` vs `crates/`
binary/library split that never had a second member. Renamed to `atomic`
(the product's actual name, see `CLAUDE.md`'s "Project naming" section)
and moved under `crates/` so the three real scopes of this repo line up as
three top-level directories: `crates/atomic` (this crate), `crates/atomicjs`
(experimental JS compiler), `crates/neutron/*` (render/execute engine).
See `spec/proposals/NEUTRON_ENCAPSULATION.md`.
