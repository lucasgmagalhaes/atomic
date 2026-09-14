# Plan: real live document.forms/images/links/scripts

**Complexity**: Small

## Summary

Follow-up to PR #139 (live `getElementsByTagName`/`getElementsByClassName`
via a real `Proxy`, `crates/js-runtime/src/dom_bindings/collections/
live.rs`). Extends the same `live.rs` machinery to `document.forms`
(`<form>`), `document.images` (`<img>`), `document.scripts` (`<script>`)
— all three are already plain tag queries (`document_elements_by_tag` in
`document_properties.rs`), so they reuse the existing `Query::Tag`
variant via `live_elements_by_tag_name(ctx, root, tag, true)` (`true` =
`include_start`, matching `document_get_elements_by_tag_name`'s own
convention since the query root is the document itself). `document.links`
needs a new `Query::Links` variant — real spec union of `<a href>` and
`<area href>` elements, not a plain tag match.

## Files
| File | Action |
|---|---|
| `crates/js-runtime/src/dom_bindings/collections/live.rs` | UPDATE - `Query::Links` variant |
| `crates/js-runtime/src/dom_bindings/document_properties.rs` | UPDATE - forms/images/links/scripts call live builders |
| `crates/js-runtime/tests/live_html_collection_test.rs` | UPDATE - liveness tests for all four |
| `spec/matrix/dom.md` | UPDATE |

## Validation
Same cargo build/test/fmt sequence as prior items this session.
