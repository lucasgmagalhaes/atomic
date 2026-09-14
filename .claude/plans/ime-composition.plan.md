# Plan: real IME/composition events

**Complexity**: Small-Medium

## Summary

Last remaining item on `spec/matrix/events.md` line 17. New
`CompositionEvent` native class (`.data` string field, chained onto
`Event.prototype` - same convention every other event subclass this
session's `event_subclasses/` already uses), plus three new host
commands operating on whichever id `KEY`/`CLICK_AT` last focused:
`COMPOSITION_START`, `COMPOSITION_UPDATE <text>`, `COMPOSITION_END
<text>`.

- `COMPOSITION_START`: dispatches real `"compositionstart"`
  (`data: ""`, bubbles, cancelable - real spec shape).
- `COMPOSITION_UPDATE <text>`: dispatches real `"compositionupdate"`
  (`data: text`, bubbles, not cancelable) - no field mutation, matches
  real IME preview-only behavior mid-composition.
- `COMPOSITION_END <text>`: dispatches real `"compositionend"`
  (`data: text`, bubbles, not cancelable - real spec: this one can't be
  prevented), then the real default action always runs (spec-accurate,
  unlike this session's other cancelable-gated defaults): appends `text`
  to the focused field's `.value` - that assignment itself already
  dispatches a real generic `"input"` event (`text_value.rs`'s own
  `node_value_set`, the same mechanism `type_key`'s regular character
  handling already relies on), so no separate `InputEvent` construction
  is added on top (would be a redundant double dispatch).

## Files
| File | Action |
|---|---|
| `crates/js-runtime/src/event_subclasses/composition.rs` | CREATE |
| `crates/js-runtime/src/event_subclasses/mod.rs` | UPDATE |
| `crates/profile/src/bin/profile_worker/input_commands/composition.rs` | CREATE |
| `crates/profile/src/bin/profile_worker/input_commands/mod.rs` | UPDATE |
| `crates/profile/src/bin/profile_worker/commands/input.rs` | UPDATE |
| `crates/profile/src/interaction.rs` | UPDATE - `Profile::composition_start/update/end` |
| `crates/js-runtime/tests/composition_event_test.rs` | CREATE |
| `crates/profile/tests/composition_test.rs` | CREATE |
| `spec/matrix/events.md` | UPDATE |
