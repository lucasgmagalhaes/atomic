# Plan: real live `Node.childNodes`

**Complexity**: Small

## Summary

Last real liveness gap `spec/matrix/dom.md`'s Collections line still
names: `Node.childNodes` is real spec-live (unlike `querySelectorAll`,
whose real spec return type `NodeList` is static - confirmed, no work
needed there). Extends `collections/live.rs`'s `Proxy` machinery
(from PRs #139/#140) with a `Query::Children` variant that re-reads
`dom::Dom::get(id).children` on every access instead of the current
one-shot `node.children.clone()` snapshot (`navigation.rs`'s
`node_children_get`).

## Real shape difference from HTMLCollection
Real `NodeList` only gets `item(index)`, no `namedItem` -
`node_list.rs`'s own snapshot wrapper already reflects this. `live.rs`'s
existing `get`/`has` traps unconditionally exposed `namedItem` (fine
when every prior caller was HTMLCollection-shaped) - adds a
`named_item: bool` flag to `LiveCollectionState` so a live `NodeList`
genuinely doesn't expose `namedItem`, matching real spec.

## Files
| File | Action |
|---|---|
| `crates/js-runtime/src/dom_bindings/collections/live.rs` | UPDATE - `Query::Children`, `named_item` flag |
| `crates/js-runtime/src/dom_bindings/navigation.rs` | UPDATE - `node_children_get` calls the live builder |
| `crates/js-runtime/tests/live_html_collection_test.rs` | UPDATE - childNodes liveness test |
| `spec/matrix/dom.md` | UPDATE |
