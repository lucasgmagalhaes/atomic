# Plan: MOUSE_MOVE host command + real mouseover/mouseout dispatch

**Complexity**: Small

## Summary

`spec/matrix/events.md` line 17 ("Pointer capture, hover/enter/leave,
double-click, context menu, drag-and-drop, clipboard events and
IME/composition") is genuinely `[ ]` — confirmed via Explore agent, not a
stale doc line like two other items fixed earlier this session. `dom::Dom`
already has real `:hover` state tracking (`crates/dom/src/hover.rs`'s
`set_hovered`/`clear_hover`/`hovered_element`, bumping `style_version`) but
it is *never called* from any real host input path — only referenced in
doc comments in `profile-worker`. There is currently no mouse-move-shaped
host command at all (`grep` across `commands/` and `input_commands/`
confirms only `CLICK`/`CLICK_AT`/`KEY`/`TAB`/`TAB_REVERSE`/`FILL`/`SCROLL`).

This plan scopes the smallest real slice: a new `MOUSE_MOVE x y` host
command that hit-tests via `Page::hit_test_at` (same primitive `CLICK_AT`
already uses), updates `dom::Dom`'s hover state, and dispatches real
bubbling `mouseout`/`mouseover` `MouseEvent`s on the nearest id-addressable
ancestor of the old/new hover target (same real limitation
`dispatch_click_at` already documents on `nearest_id_ancestor`).

## Scope cuts (documented, not silently dropped)

- `relatedTarget` stays `null` on the dispatched events — `Context`'s only
  entry point is `eval(source: &str)` (see `js_string_literal`'s own doc),
  so cross-referencing two live JS element objects inside one generated
  eval string isn't supported without a new binding. A future increment
  could add a `Context` method that takes a `NodeId` argument directly.
- Non-bubbling `mouseenter`/`mouseleave` are NOT included — they need an
  ancestor-*set* diff (fire on every ancestor in old chain not in new
  chain, and vice versa), not a single-node dispatch like `mouseover`/
  `mouseout`. Left `[ ]`.
- Pointer capture, dblclick, contextmenu, drag-and-drop, clipboard events,
  IME/composition are NOT touched by this plan — each needs its own new
  host command and/or JS-facing wiring. Line 17 stays partially `[ ]`
  after this lands; the matrix note will spell out exactly what's done vs
  still open.

## Files to change

| File | Action | Why |
|---|---|---|
| `crates/profile/src/bin/profile_worker/input_commands/mouse_move.rs` | CREATE | `dispatch_mouse_move` — hit-test, hover state update, mouseout/mouseover dispatch |
| `crates/profile/src/bin/profile_worker/input_commands/mod.rs` | UPDATE | export new fn |
| `crates/profile/src/bin/profile_worker/commands/input.rs` | UPDATE | `MOUSE_MOVE x y` command parsing/dispatch, mirrors `CLICK_AT` |
| `spec/matrix/events.md` | UPDATE | line 17 note: mouseover/mouseout done, rest still open |

## Validation

```bash
cargo test -p profile
cargo build --workspace --exclude shell
cargo test --workspace
cargo fmt --all
```

## Acceptance

- [ ] `MOUSE_MOVE x y` command hit-tests and updates `dom::Dom` hover state
- [ ] Real bubbling `mouseout`/`mouseover` dispatch on id-addressable ancestors
- [ ] Tests cover: hover set/clear, mouseover fires on enter, mouseout fires on leave, no redundant dispatch when staying over the same element
- [ ] matrix + workspace build/test green, fmt clean
