# Plan: real drag-and-drop (dragstart/dragover/drop/dragend)

**Complexity**: Medium

## Summary

Last item on `spec/matrix/events.md` line 17. Two new coordinate-driven
host commands, mirroring `CLICK_AT`/`MOUSE_MOVE`'s hit-test convention:

- `DRAG_START x y`: hit-tests the source, dispatches a real cancelable
  `"dragstart"` `DragEvent` with a working `.dataTransfer`
  (`setData`/`getData` closures - single `"text/plain"` format, same
  scope cut `dispatch_copy_or_cut`'s clipboardData already documents).
  If not canceled, remembers `(source_id, data)` in a new
  `WorkerState::drag_source` field. If canceled, no drag starts (real
  spec: `preventDefault()` on `dragstart` cancels the whole drag).
- `DROP_AT x y`: requires a drag already in progress (`Err` otherwise).
  Hit-tests the target, dispatches a real cancelable `"dragover"`
  `DragEvent` there. Real spec: only a `dragover` listener that calls
  `preventDefault()` marks a valid drop target - so `drop` only fires
  when `dragover` was canceled; otherwise this reports "not a valid drop
  target" without erroring (a real, valid outcome). When it does fire:
  dispatches `"drop"` (cancelable, `dataTransfer.getData` returns the
  real stored text) on the target, then `"dragend"` (bubbles, not
  cancelable - real spec) on the source, then clears `drag_source`.

## Scope cuts
- Single `"text/plain"` format only (`dataTransfer.setData`/`getData`),
  same as `clipboardData`.
- No `"drag"` (repeated, during-drag) or `"dragenter"`/`"dragleave"`
  events - those need continuous coordinate tracking mid-drag
  (`mouseenter`/`mouseleave`'s own ancestor-set-diff shape), out of
  scope for this first real increment. `dragover` alone (fired once per
  `DROP_AT` call, not continuously) is real but simplified.
- No drag image/`effectAllowed`/`dropEffect` modeling.

## Files
| File | Action |
|---|---|
| `crates/profile/src/bin/profile_worker/input_commands/drag.rs` | CREATE |
| `crates/profile/src/bin/profile_worker/input_commands/mod.rs` | UPDATE |
| `crates/profile/src/bin/profile_worker/commands/mod.rs` | UPDATE - `drag_source` field |
| `crates/profile/src/bin/profile_worker/commands/input.rs` | UPDATE |
| `crates/profile/src/bin/profile_worker/main.rs` | UPDATE - init field |
| `crates/profile/src/interaction.rs` | UPDATE - `Profile::drag_start`/`drop_at` |
| `crates/profile/tests/drag_drop_test.rs` | CREATE |
| `spec/matrix/events.md` | UPDATE |
