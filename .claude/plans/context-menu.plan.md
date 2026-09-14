# Plan: real `contextmenu` dispatch

**Complexity**: Small

## Summary

`spec/matrix/events.md` line 17's "context menu" item. New `CONTEXT_MENU_AT
x y` host command: hit-tests via `Page::hit_test_at` (same primitive
`CLICK_AT`/`MOUSE_MOVE` use), dispatches a real bubbling, cancelable
`"contextmenu"` `MouseEvent` (`button: 2`) on the nearest id-addressable
ancestor. No native context menu exists in this engine to suppress, so
there's no real default action to gate on `preventDefault()` - the event
itself, and a page's own listener reacting to it, is the real surface.

## Scope cut
No focus-movement side effect (unlike `CLICK_AT`'s real "focus the hit
input" behavior) - a real right-click's focus effect is
platform-dependent and this engine has no native menu to model it
against.

## Files
| File | Action |
|---|---|
| `crates/profile/src/bin/profile_worker/input_commands/context_menu.rs` | CREATE |
| `crates/profile/src/bin/profile_worker/input_commands/mod.rs` | UPDATE |
| `crates/profile/src/bin/profile_worker/commands/input.rs` | UPDATE |
| `crates/profile/src/interaction.rs` | UPDATE - `Profile::context_menu_at` |
| `crates/profile/tests/context_menu_test.rs` | CREATE |
| `spec/matrix/events.md` | UPDATE |
