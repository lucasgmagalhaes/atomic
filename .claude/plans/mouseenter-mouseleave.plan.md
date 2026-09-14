# Plan: mouseenter/mouseleave (ancestor-set diff)

**Complexity**: Small

## Summary

Closes the "hover/enter/leave" remainder of `spec/matrix/events.md` line
17 left open by PR #127 (real `mouseover`/`mouseout`). Non-bubbling
`mouseenter`/`mouseleave`: for every id-addressable ancestor of the
previous hover target no longer in the new hover target's id-ancestor
chain, fire `mouseleave`; for every id-addressable ancestor of the new
target not in the old chain, fire `mouseenter`. Both non-bubbling,
non-cancelable, dispatched directly at each element (not via the target's
own bubble path).

## Scope cut (unchanged from #127)

Still id-addressable-only (same limitation `nearest_id_ancestor` already
documents for `CLICK_AT`/`mouseover`/`mouseout`): only ancestors that
themselves carry a real `id` attribute are reachable, since dispatch goes
through a generated `getElementById` eval string. `relatedTarget` stays
`null`.

## Files to change

| File | Action | Why |
|---|---|---|
| `crates/profile/src/bin/profile_worker/input_commands/mouse_move.rs` | UPDATE | id-ancestor-chain diff, mouseleave/mouseenter dispatch |
| `crates/profile/tests/hover_events_test.rs` | UPDATE | nested-element enter/leave tests |
| `spec/matrix/events.md` | UPDATE | line 17 note |

## Validation

```bash
cargo test -p profile --test hover_events_test
cargo test -p profile
cargo build --workspace --exclude shell
cargo test --workspace --exclude shell
cargo fmt --all
```
