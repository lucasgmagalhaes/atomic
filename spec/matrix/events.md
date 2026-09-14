# Events & Input — Capability Status

## 3. Events and input

**Done (55%)**

- [x] Stable element identity across lookups.
- [x] Ordered listeners, listener removal, listener count cap and exception isolation.
- [x] Event construction and option flags.
- [x] Target phase and bounded bubbling; cancellation and propagation stop.
- [x] Native profile-worker dispatch for click, keyboard input, focus, blur, input and change.

**Needed**

- [ ] Capturing phase, `once`, `passive`, `signal`, listener objects and full event path/composed semantics.
- [x] Event subclasses: ~~`MouseEvent`~~, ~~`PointerEvent`~~, ~~`KeyboardEvent`~~, ~~`InputEvent`~~, ~~`FocusEvent`~~, ~~`SubmitEvent`~~, ~~`CustomEvent`~~, ~~`WheelEvent`~~, ~~`TouchEvent`~~, ~~`DragEvent`~~ — `CustomEvent`/`KeyboardEvent`/`PointerEvent` done in an earlier pass (see "Next implementation order" item 4); `MouseEvent` (`screenX`/`screenY`/`clientX`/`clientY`/`ctrlKey`/`shiftKey`/`altKey`/`metaKey`/`button`/`buttons`/`relatedTarget`) and `FocusEvent` (`relatedTarget`) done (2026-08-25, `crates/js-runtime/src/event_subclasses.rs`), same pattern as the earlier three: plain own data properties on top of `events::create_event`'s base `Event` object, prototype chained onto `Event.prototype` so `instanceof Event` holds. `relatedTarget` is the first field on any of these subclasses that's a `Node` reference rather than a primitive — read via a new `read_value_option` helper (no coercion, defaults to `null`) and written with a new `set_value_prop` that transfers ownership of the already-duped `JSValue` straight into `JS_SetPropertyStr`, same convention `detail`'s object-valued handling in `custom_event_constructor` already used ad hoc. `InputEvent` (`data`/`inputType`/`isComposing`) done (2026-08-25, same file): `data` reuses `relatedTarget`'s `read_value_option`/`set_value_prop` pair since the real API allows `data` to be `null` (composition/deletion events carry no string), not a primitive-only field like `inputType`/`isComposing`. ~~`WheelEvent`~~ — done (2026-08-25, see "Next implementation order" item 11 for the full writeup): `deltaX`/`deltaY`/`deltaZ`/`deltaMode`/`clientX`/`clientY`. Still `[ ]`: `SubmitEvent`, `TouchEvent`, `DragEvent`.
- [ ] Pointer capture, hover/enter/leave, double-click, context menu, drag-and-drop, clipboard events and IME/composition.
- [x] Default actions: links, buttons, forms, focus navigation, selection and scrolling — see `spec/ROADMAP.md` items 20/22 for the full writeup and scope cuts (still `[ ]`: `scrollIntoView()`, `behavior: smooth`, horizontal scroll — see item 22's own note).

---

[← back to spec/INDEX.md](../INDEX.md)
