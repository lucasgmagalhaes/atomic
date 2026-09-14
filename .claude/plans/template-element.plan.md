# Plan: `<template>` element (real `.content` fragment)

**Complexity**: Small

## Summary
Closes the last named gap in `spec/matrix/dom.md`'s custom-elements/shadow-DOM/mutation-observer cluster: `<template>` elements. Today `crates/html/src/sink.rs`'s `get_template_contents` just returns the `<template>` element itself, so its children live as normal, visible DOM children instead of an isolated, inert `DocumentFragment` reachable via `.content`. `html5ever`'s own tree builder (`tree_builder/mod.rs:426-429/440-442`, confirmed by reading the vendored crate source) already redirects every insertion under a `<template>` through `TreeSink::get_template_contents` — the only work needed is making that method return a real, separate fragment instead of the template itself, then exposing `.content` on the JS side.

No new `dom::NodeData` variant needed: `DocumentFragment` already exists as a bare unit variant (`crates/dom/src/lib.rs:151`), created via `Dom::create_document_fragment` (`crates/dom/src/mutation.rs:65-66`) — exactly the right shape.

## Patterns to Mirror
| Category | Source | Pattern |
|---|---|---|
| Node data field | `crates/dom/src/lib.rs:137-147` (`shadow_root: Option<NodeId>`) | Add `template_content: Option<NodeId>` to `NodeData::Element`, same "always present, `None` unless relevant tag" shape `shadow_root`/`value`/`checked` already use |
| Element subclass registration | `crates/js-runtime/src/dom_bindings/node_registry.rs` (`HTML_FORM_CLASS_KIND` + tag match in `node_class_id_for`), `element_classes/classes.rs::ensure_html_subclass` (`if kind == HTML_FORM_CLASS_KIND { define_form_properties(...) }` chain) | Add `HTML_TEMPLATE_CLASS_KIND`, a `"template"` tag match arm, and a `define_template_properties` branch adding the `.content` getter |
| Fragment JS wrapper | `document_create_document_fragment` (`crates/js-runtime/src/dom_bindings/document_creation.rs:110-123`) — wraps a `DocumentFragment` `NodeId` via `node_class_id_for`/`node_object` | `.content` getter does the same conversion for the stashed fragment `NodeId` |

## Files to Change
| File | Action | Why |
|---|---|---|
| `crates/dom/src/lib.rs` | UPDATE | Add `template_content: Option<NodeId>` field to `NodeData::Element` |
| `crates/dom/src/mutation.rs` | UPDATE | `create_element`: if `tag == "template"`, eagerly create and stash a real content `DocumentFragment` (matches real spec: a `<template>` always owns a content fragment from creation, not lazily) |
| `crates/dom/src/lib.rs` (or a small new fn near `Dom::create_element`) | UPDATE | Add `Dom::template_content(&self, id: NodeId) -> Option<NodeId>` accessor |
| `crates/html/src/sink.rs` | UPDATE | `get_template_contents`: return `self.dom.borrow().template_content(*target).expect(...)` instead of `*target`; update/remove the module-doc scope-cut note for `<template>` |
| `crates/js-runtime/src/dom_bindings/node_registry.rs` | UPDATE | Add `HTML_TEMPLATE_CLASS_KIND` + `"template"` tag arm |
| `crates/js-runtime/src/dom_bindings/element_classes/classes.rs` | UPDATE | Wire `HTMLTemplateElement` prototype + `.content` getter |
| `crates/dom/tests/dom_test.rs` | UPDATE | Unit test: `create_element("template")` has a distinct, empty content fragment; appending children to the template via `append_child` (simulating what the parser does after `get_template_contents` redirection) lands them in the fragment, not the template's own `children` |
| `crates/html/tests/*` (existing html test file) | UPDATE | Integration test: parsing `<template><p>hi</p></template>` — the template element has no light-DOM children, its content fragment has one `<p>` child |
| `crates/js-runtime/tests/*` (new or existing dom_bindings test file) | CREATE/UPDATE | `document.createElement('template').content` is a real `DocumentFragment` (`nodeType === 11`), starts empty, and is genuinely isolated from the template's own `childNodes` |
| `spec/matrix/dom.md` | UPDATE | Flip the `<template>` bullet from `[ ]` to done, documenting the scope cut below |

## Tasks
### Task 1: `dom` crate — content fragment field + accessor
- **Action**: add `template_content` field to `Element`, initialize `None` in the one literal constructor site (`mutation.rs:46`), add eager creation for `tag == "template"`, add `Dom::template_content` getter.
- **Mirror**: `shadow_root`'s field shape and its accessor pattern.
- **Validate**: `cargo test -p dom`.

### Task 2: `html` crate — real template-contents redirection
- **Action**: `get_template_contents` returns the stashed fragment; update the module doc's scope-cut list.
- **Mirror**: existing `TreeSink` method bodies in the same file.
- **Validate**: `cargo test -p html`.

### Task 3: `js-runtime` — `HTMLTemplateElement.content`
- **Action**: new class kind + tag dispatch + `.content` getter (read-only, no setter — real spec's `.content` is also read-only).
- **Mirror**: `HTMLFormElement`/`HTMLSelectElement`'s `ensure_html_subclass` branch shape.
- **Validate**: `cargo test -p js-runtime --release -- --test-threads=1`.

## Validation
```bash
cargo test -p dom --release -- --test-threads=1
cargo test -p html --release -- --test-threads=1
cargo test -p js-runtime --release -- --test-threads=1
cargo build --workspace --exclude shell
cargo test --workspace --exclude shell --exclude automation --release -- --test-threads=1
cargo fmt --all
```

## Risks
| Risk | Likelihood | Mitigation |
|---|---|---|
| A nested `<template><template>...</template></template>` — each level needs its own independent content fragment | Low | Eager creation happens per `create_element` call, so nesting naturally gets independent fragments; add a test |
| `cloneNode(deep)` on a `<template>` — real spec clones the content fragment too | Medium | Out of scope for this pass (documented deviation, same class as `shadow_root` not being cloned) — note it in the matrix update |
| `document.createElement('template').content` breaking existing generic `Node`-only assumptions in `dom_bindings` (e.g. serialization, `childNodes`) | Low | Fragment is a fully ordinary `DocumentFragment` node like `createDocumentFragment()` already produces — no special-casing needed elsewhere |

## Acceptance
- [ ] All tasks complete
- [ ] Validation passes
- [ ] `<template>` content is real and isolated; scope cut (`cloneNode` doesn't clone content) documented, not silently wrong
