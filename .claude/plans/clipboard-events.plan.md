# Plan: real copy/cut/paste dispatch via KEY

**Complexity**: Small-Medium

## Summary

`spec/matrix/events.md` line 17's "clipboard events" item. `KEY` already
types literal characters into the focused id (`keyboard.rs`'s
`type_key`). Extends it with three recognized combo strings -
`"Ctrl+C"`, `"Ctrl+X"`, `"Ctrl+V"` - that dispatch real cancelable
`"copy"`/`"cut"`/`"paste"` `Event`s (plain `Event`, not a new
`ClipboardEvent` native class - a plain JS `clipboardData` object with
`getData`/`setData` closures is built inline in the generated eval
script instead, same "runtime-generated small snippet" convention this
file and `click.rs` already use) with a real, working `.clipboardData`,
wired to the real OS clipboard via `platform_apis::clipboard_write_text`/
`clipboard_read_text` - the same crate `js-runtime`'s own
`navigator.clipboard` already uses, just called directly from host Rust
instead of through an async JS Promise (this protocol's `KEY` round trip
is synchronous, so a Promise-based path would reply before it resolves).

## Real default actions
- `copy`/`cut`: if not canceled and the listener didn't call `setData`
  itself, defaults to copying the *entire* focused field's `.value` (no
  partial-selection copy - matches this engine's documented "no
  user-driven selection" scope cut elsewhere). `cut` additionally clears
  the field's `.value` after a successful, uncanceled dispatch.
- `paste`: if not canceled, appends the real OS clipboard text to the
  field's `.value` (append, not cursor-position insert - same convention
  regular typed characters already use in `type_key`).

## Files
| File | Action |
|---|---|
| `crates/profile/Cargo.toml` | UPDATE - add `platform-apis` dep |
| `crates/profile/src/bin/profile_worker/input_commands/keyboard.rs` | UPDATE |
| `crates/profile/tests/clipboard_events_test.rs` | CREATE |
| `spec/matrix/events.md` | UPDATE |

## Validation
Same cargo build/test/fmt sequence as prior items this session. Note: OS
clipboard tests can be flaky/unavailable under a headless CI runner (see
`ci.yml`'s own existing skip for the real clipboard round-trip test in
`js-runtime`) - keep new tests scoped to what's locally verifiable, flag
if CI needs the same skip.
