# Plan: `Event.composed`/`Event.composedPath()`

**Complexity**: Small

## Summary
`spec/matrix/events.md` line 15 lists "Capturing phase, `once`, `passive`, `signal`, listener objects and full event path/composed semantics" as `[ ]`. Investigation (via an Explore agent) found capturing/`once`/`passive`/`signal`/listener-objects are already fully implemented and tested (`crates/js-runtime/src/events/dispatch/`, `listeners.rs`, `abort_controller/`) — only `Event.composed`/`Event.composedPath()` are genuinely missing. This plan closes just that real gap.

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Event option field | `event_class.rs`'s `bubbles`/`cancelable` (read in `event_constructor`, stored on `EventState`, exposed via a `Getter`) | `composed` gets the identical shape |
| Per-dispatch mutable state | `EventState.current_target`, set in `chain.rs::run_phases`'s `run_at` closure | `EventState.path: Vec<dom::NodeId>`, set once per `dispatch`/`dispatch_existing` call before `run_phases` runs |
| Method returning an array of Node objects | none exact, but `dom_bindings::node_object`/`node_class_id_for` conversion is used identically by `chain.rs`'s own `run_at` | `composed_path` builds a `JS_NewArray` of `node_object(...)` calls over `path` |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/js-runtime/src/events/event_class.rs` | UPDATE | Add `composed: bool` + `path: Vec<dom::NodeId>` to `EventState` (both default `false`/empty); read `"composed"` in `event_constructor` the same way as `bubbles`/`cancelable`; add `composed` getter and `composedPath` method |
| `crates/js-runtime/src/events/dispatch/chain.rs` | UPDATE | `run_phases` sets `(*state(ctx, event)).path = chain.to_vec()` once, before running the three phases |
| `crates/js-runtime/tests/event_subclasses_test.rs` or a small new test file | UPDATE/CREATE | `composedPath()` returns the real target→root chain during a real DOM dispatch; empty array before dispatch and for a "simple" (non-`Node`) target like `window` |
| `spec/matrix/events.md` | UPDATE | Flip line 15 to done, with the scope cut: `composedPath()` only returns a real chain for `Node`-tree dispatch (`dispatchEvent` on an actual node) — a "simple" target (`window`/`document`/`history`, no DOM tree to walk) always returns `[]`, matching this file's own existing capture/bubble scope split |

## Tasks
### Task 1: `EventState` + constructor + getters
- **Action**: add fields, read `composed` option, add `composed`/`composedPath` on `Event.prototype`.
- **Validate**: `cargo build -p js-runtime`.

### Task 2: populate `path` during real dispatch
- **Action**: `run_phases` sets `path` from `chain` once at the top.
- **Validate**: `cargo test -p js-runtime --release -- --test-threads=1`.

### Task 3: tests + matrix update
- **Validate**: full workflow (`cargo build --workspace --exclude shell`, full test suite, `cargo fmt --all`).

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| Real spec clears `composedPath()` back to `[]` once dispatch finishes (dispatch flag unset) | Medium | Documented deviation: this engine keeps `path` populated after dispatch completes too, simpler than tracking a live "currently dispatching" flag with no real consumer needing the stricter behavior |
| Shadow-DOM-crossing semantics of `composed` (path continues past a shadow boundary) | Low | Out of scope — this engine's shadow DOM has no `<slot>`/render integration already (`ROADMAP.md` item 38's own scope cut), so `composed`'s only real effect here is the boolean property itself |

## Acceptance
- [ ] All tasks complete
- [ ] Validation passes
- [ ] `composed`/`composedPath()` real for Node-tree dispatch; scope cut (simple targets, post-dispatch retention) documented
