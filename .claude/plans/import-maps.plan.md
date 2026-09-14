# Plan: real import maps

**Complexity**: Small-Medium

## Summary

`spec/matrix/runtime.md` line 18's last remaining real sub-item — the
rest of that line (ES-module loader/resolver, dynamic `import()`, module
cache) turned out already real and tested (`es_modules_test.rs`,
`module_loader_test.rs`), just left mis-marked; a separate doc PR fixes
that. This plan is the one genuine gap: a bare specifier (`import
"lodash"`, no scheme, not `./`/`../`/`/`) currently can't resolve at all
(`module_loader::resolve_specifier`'s own documented cut).

Real scope: `Context::set_import_map(json: &str) -> Result<(), String>`
parses `{"imports": {"bare": "url", "prefix/": "url/"}}` via
`JS_ParseJSON` (the dedicated C API `CLAUDE.md`'s own gotcha calls for,
not a re-entrant `JS_Eval`) and `JS_GetOwnPropertyNames` (same real
enumeration `value_bridge.rs`'s `js_object_to_value` already uses),
stored on `HostState`. `resolve_specifier` gains a `ctx` parameter so it
can consult the map for a bare specifier: exact match first, then
longest-prefix match for a `"prefix/"`-shaped key. Both real callers
(`Context::resolve_module_specifier`, `module_normalize_fn` — the actual
`JSModuleNormalizeFunc` QuickJS invokes for every static/dynamic
`import`) already have `ctx` in scope.

## Scope cuts
No `scopes` (real spec's per-URL-prefix override maps) — one flat
`imports` map only. No "only one import map per document, must precede
every module script" enforcement (real spec forbids a second one after
module execution starts) — `set_import_map` can be called any number of
times, last write wins, same "host controls timing, no window into a
concurrent execution" trust boundary every other host-facing setter
here has.

## Files
| File | Action |
|---|---|
| `crates/js-runtime/src/host_state.rs` | UPDATE - `import_map` field |
| `crates/js-runtime/src/import_map.rs` | CREATE - `set_import_map`, resolution logic |
| `crates/js-runtime/src/module_loader.rs` | UPDATE - `resolve_specifier` takes `ctx`, consults map |
| `crates/js-runtime/src/context/mod.rs` | UPDATE - `Context::set_import_map`, init field |
| `crates/js-runtime/src/lib.rs` | UPDATE - `mod import_map;` |
| `crates/js-runtime/tests/import_map_test.rs` | CREATE |
| `spec/matrix/runtime.md` | UPDATE |
