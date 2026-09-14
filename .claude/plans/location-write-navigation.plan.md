# Plan: real `location.href = ...`/`assign`/`replace`/`reload`

**Complexity**: Small

## Summary

`spec/matrix/navigation-security.md` line 20. This engine already has a
real "JS requests navigation, host performs it" channel
(`host_state::HostState::pending_navigation`, consumed by
`profile-worker`'s `navigate_if_requested` after any command that could
run a click handler) - currently only fed by a real `<a href>` click's
default action (`dom_bindings::forms::run_default_click_action`).

Wires `location.href`'s setter, `location.assign(url)`,
`location.replace(url)`, and `location.reload()` through that exact same
mechanism: each just writes the raw (possibly relative) URL string into
`pending_navigation` - the existing host-side `resolve_url` already
handles relative resolution, so no new logic needed there.

## Scope cut
`assign`/`replace` request the identical real navigation - this engine's
`pending_navigation` always triggers a fresh top-level `NAVIGATE`, with
no distinct "replace this history entry" vs "push a new one" treatment
(same-document `history.pushState`/`replaceState` already handle that
side genuinely - see `crate::history` - but a real cross-document
navigation here doesn't yet interact with that history stack at all).
`reload()` re-requests the current URL.

Same existing limitation as every other `pending_navigation` writer:
only consumed after a command that calls `navigate_if_requested`
(`CLICK`/`CLICK_AT`/`EVAL`/etc, per `commands/input.rs`'s own call
sites) - setting `location.href` from, say, a bare `setTimeout` callback
with no subsequent command won't navigate until the next such command
runs. Pre-existing, not new to this change.

## Files
| File | Action |
|---|---|
| `crates/js-runtime/src/location.rs` | UPDATE |
| `crates/js-runtime/tests/location_navigation_test.rs` | CREATE |
| `spec/matrix/navigation-security.md` | UPDATE |
