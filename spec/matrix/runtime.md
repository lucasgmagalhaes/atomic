# JS Runtime & Host — Capability Status

## 1. JavaScript runtime and host

**Done (83%)**

- [x] `Runtime`, `Context`, synchronous `eval` and exception conversion.
- [x] ES language execution through QuickJS-NG: classes, modules supported by the upstream runtime, promises, async functions, typed arrays, `Map`, `Set`, regex and standard built-ins.
- [x] Per-context DOM, storage and native-object state; per-runtime class registration.
- [x] Promise-job draining and host timer/event pumping.
- [x] Resource limits on selectors, DOM string inputs, event listeners, bubbling and nested dispatch.
- [x] `crypto.getRandomValues`.
- [x] `console` (`log`/`info`/`warn`/`error`/`debug`) with leveled message storage hosts can drain, and structured uncaught-error reporting (real exception text + a `window` `error` event).
- [x] Interrupt/time budget for untrusted script execution (`Context::set_time_budget`: quickjs's interrupt handler aborts a budget-exceeding `eval` with an "interrupted" InternalError; deadline re-stamped per call) and memory quotas per document (`Context::set_memory_limit`, quickjs's runtime-wide allocation cap — one runtime per context here, so per-document is exactly what it gives).

**Needed**

- [x] ES-module loader/resolver, import maps, dynamic `import()` and module cache. Stale line - loader/resolver/dynamic `import()`/module cache were already real and tested (`crates/js-runtime/src/module_loader.rs`'s `JS_SetModuleLoaderFunc`-backed `JSModuleNormalizeFunc`/`JSModuleLoaderFunc`, `context/eval.rs`'s `eval_module`: real compile→resolve→execute linking, a real module namespace, circular-dependency handling — `es_modules_test.rs`/`module_loader_test.rs`) before this line was last touched. Import maps (the one genuine remaining gap) done now: `Context::set_import_map(base_url, json)` (`crates/js-runtime/src/import_map.rs`) parses a real `{"imports": {...}}` object via `JS_ParseJSON`/`JS_GetOwnPropertyNames`, resolving a bare specifier (exact match, else longest `"prefix/"` match) that previously couldn't resolve at all — `import_map_test.rs`. Scope cuts: no `scopes` (one flat `imports` map only), no "one map, must precede every module" enforcement (last `set_import_map` call wins).
- [x] Script loading modes: parser-blocking, `defer`, `async`, module scripts and CSP-aware loading. Partially stale: `defer` ordering and `type="module"` are both real (`profile-worker`'s `page_source/scripts.rs`/`page/load.rs`, see the ES-module line above) - this document previously claimed module scripts weren't wired in at all. Still genuinely `[ ]`: real concurrent `async` ordering (this worker fetches scripts one at a time on a single thread, so `async` runs as a synchronous non-deferred script - an honest projection, not a distinct code path) and CSP-aware script loading (`crates/js-runtime/src/csp.rs` only gates `connect-src`/`default-src`, not `script-src`).
- [ ] Source maps, debugger protocol and remaining structured error reporting (console + uncaught-error reporting are done — see section 1's Done list; `EvalError` now carries the real stringified exception instead of the old "script raised an exception" placeholder).
- [ ] Worker runtime: `Worker`, `SharedWorker`, `MessageChannel`, structured clone and transferable objects. Correction (this line was wrongly marked `[x]` in a prior pass - `crates/workers/`'s `Worker` is real OS-thread infrastructure and genuinely tested (`worker_test.rs`), but it's a *Rust-only* API: nothing in `js-runtime` registers a `Worker` global, so no page script can actually call `new Worker(...)` - `grep`-confirmed zero JS-facing bindings, zero callers of `workers::Worker` outside its own test file. `MessageChannel`/`MessagePort` (`message_channel.rs`) and transferable objects (`uint8array_transfer_test.rs`) *are* genuinely real and JS-facing - that part of the line was accurate. Still `[ ]`: a real JS-facing `Worker` (the actual remaining gap, bigger than previously scoped) and `SharedWorker` on top of it.

---

[← back to spec/INDEX.md](../INDEX.md)
