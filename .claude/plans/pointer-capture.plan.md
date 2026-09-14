# Plan: real setPointerCapture/releasePointerCapture/hasPointerCapture

**Complexity**: Small

## Summary

`spec/matrix/events.md` line 17's "pointer capture" item. Adds real
`Element.setPointerCapture`/`releasePointerCapture`/`hasPointerCapture`
on `Node.prototype`, backed by a new `dom::Dom` field
(`pointer_captures: HashMap<i32, NodeId>`, mirroring `hovered`/`focused`'s
shape but keyed by pointer id). `setPointerCapture` fires real
`"gotpointercapture"`; `releasePointerCapture` fires real
`"lostpointercapture"` only when a real release actually happened (real
spec: releasing an uncaptured pointer is a no-op).

## Scope cut
This engine dispatches no real host-driven `PointerEvent`s at all yet
(confirmed: `MOUSE_MOVE`'s own real dispatch is `mouseover`/`mouseout`/
`mouseenter`/`mouseleave`, not pointer events - see
`profile-worker/input_commands/mouse_move.rs`), so capture has nothing to
redirect *to*. What's real here is the bookkeeping API itself and its
events - useful to a page that calls `setPointerCapture` from a real
`mousedown`-equivalent handler and later checks `hasPointerCapture`.

## Files
| File | Action |
|---|---|
| `crates/dom/src/pointer_capture.rs` | CREATE |
| `crates/dom/src/lib.rs` | UPDATE - field + mod |
| `crates/js-runtime/src/dom_bindings/pointer_capture.rs` | CREATE |
| `crates/js-runtime/src/dom_bindings/mod.rs` | UPDATE |
| `crates/js-runtime/src/dom_bindings/element_classes/classes.rs` | UPDATE - wire into `ensure_node_class` |
| `crates/js-runtime/tests/pointer_capture_test.rs` | CREATE |
| `spec/matrix/events.md` | UPDATE |
