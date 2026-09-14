# Plan: real `dblclick` dispatch

**Complexity**: Small

## Summary

`spec/matrix/events.md` line 17's "double-click" item. `CLICK_AT` already
dispatches a real `"click"` on the nearest id-addressable ancestor
(`click.rs`'s `dispatch_click_at`). This adds real `"dblclick"`: tracks
the last click's target id + `Instant` on `WorkerState` (new
`last_click: Option<(String, Instant)>` field), and if a second
`CLICK_AT` lands on the *same* id-addressable ancestor within 500ms
(matches common OS/browser double-click thresholds - no config surface
for it here, same "one reasonable constant" convention `MAX_BUBBLE_DEPTH`
etc. already use elsewhere), dispatches a real bubbling, cancelable
`"dblclick"` `MouseEvent` right after the second `"click"`. Third click
starts a fresh sequence (state resets after a dblclick fires).

## Files to change

| File | Action |
|---|---|
| `crates/profile/src/bin/profile_worker/input_commands/click.rs` | UPDATE - `dispatch_click_at` takes `&mut Option<(String, Instant)>`, dispatches `dblclick` |
| `crates/profile/src/bin/profile_worker/commands/mod.rs` | UPDATE - add `last_click` field |
| `crates/profile/src/bin/profile_worker/commands/input.rs` | UPDATE - pass `&mut self.last_click` |
| `crates/profile/src/bin/profile_worker/main.rs` | UPDATE - init field |
| `crates/profile/tests/click_interaction_test.rs` or new test file | UPDATE/CREATE |
| `spec/matrix/events.md` | UPDATE |

## Validation
Same cargo build/test/fmt sequence as prior items this session.
