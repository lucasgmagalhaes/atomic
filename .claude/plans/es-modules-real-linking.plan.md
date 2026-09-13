# Plan: ES Modules — Real Linking/Execution + `import()` (ROADMAP item 19)

**Source**: `spec/ROADMAP.md` item 19 (free-form continuation of `.claude/plans/architecture-p5-foundations.plan.md` Stage 4, which only landed the specifier-resolver + fetch-and-cache primitive in `js_runtime::module_loader`)
**Complexity**: Large

## Summary

Give this engine real ES module semantics — `import`/`export` bindings, a module namespace object, and `import()` syntax — by registering QuickJS-ng's native module system (`JS_SetModuleLoaderFunc`) instead of building a parallel mechanism. Grounding research (reading the vendored `quickjs.h`/`quickjs.c`/`quickjs-libc.c`) confirms the full native API this needs is already vendored and unbound: `JSModuleDef`, `JSModuleLoaderFunc`/`JSModuleNormalizeFunc`, `JS_SetModuleLoaderFunc`, `JS_ResolveModule`, `JS_GetModuleNamespace`, `JS_EVAL_TYPE_MODULE`/`JS_EVAL_FLAG_COMPILE_ONLY`. Critically: **quickjs-ng routes both static `import` and dynamic `import()` through the same one loader/normalize callback pair** — there is no separate dynamic-import host hook in this version (confirmed by grep: no `JSHostImportModuleDynamically` anywhere in the header). Once the loader is registered, `import()` works automatically at the engine level; no new host-side dynamic-import plumbing is needed.

The one real scope cut this plan takes: the loader callback QuickJS calls is **synchronous** (it must return a `JSModuleDef*` or `NULL` immediately — there is no async continuation in this C API). Stage 4's existing background-thread fetch model can't satisfy that contract, so the module loader fetches synchronously (blocking `net::request`) instead — real linking/execution, but a blocking network fetch rather than Stage 4's non-blocking one. This is consistent with the engine's existing "real but narrower than spec" convention.

## Patterns to Mirror

| Category | Source | Pattern |
|---|---|---|
| FFI binding style | `crates/js-runtime/quickjs-sys/src/ffi.rs`'s existing `extern "C"` block, `types.rs`'s typedefs (`JSCFunction`, `JSClassFinalizer`) | Plain `extern "C" { pub fn ...; }` declarations plus a `pub type XFunc = unsafe extern "C" fn(...) -> ...;` typedef for each callback shape — same shape as this file's existing bindings, no new abstraction. |
| Raw pointer extraction from `JSValue` | `types.rs`'s `JSValue { u: JSValueUnion, tag: i64 }` (non-NAN-boxed layout, confirmed matching quickjs.h's non-boxed `JS_VALUE_GET_PTR` variant) | `result.u.ptr as *mut sys::JSModuleDef` — no macro needed, direct field access, matching this crate's existing raw-representation approach. |
| Eval + exception handling | `crates/js-runtime/src/context/eval.rs`'s `Context::eval`/`eval_raw` | Same `JS_Eval` → check `js_is_exception` → `JS_GetException`/stringify → `EvalError` shape; the new `eval_module` reuses this pattern for its own extra steps (`JS_ResolveModule`, `JS_EvalFunction`). |
| Per-runtime one-time setup | `crates/js-runtime/src/runtime.rs`'s `Runtime::new` | The module loader is a per-`JSRuntime` setting (`JS_SetModuleLoaderFunc(rt, ...)`), registered once in `Runtime::new`, mirroring how `class_registry` is implicitly per-runtime. |
| Existing resolver reuse | `crates/js-runtime/src/module_loader.rs`'s `resolve_specifier` (real `url::Url::join`) | The new `JSModuleNormalizeFunc` callback calls this exact function rather than reimplementing specifier resolution. |
| Tests | `crates/js-runtime/tests/*.rs`'s existing `Context::eval` integration-style tests | New tests eval real module source via the new entry point and assert on real export values / navigator-style observable side effects, same style as every other `js-runtime` test. |

## Files to Change

| File | Action | Why |
|---|---|---|
| `crates/js-runtime/quickjs-sys/src/types.rs` | UPDATE | Add `JSModuleDef` opaque type, `JSModuleLoaderFunc`/`JSModuleNormalizeFunc` typedefs, `JS_EVAL_TYPE_MODULE`/`JS_EVAL_FLAG_COMPILE_ONLY` constants. |
| `crates/js-runtime/quickjs-sys/src/ffi.rs` | UPDATE | Add `extern "C"` declarations: `JS_SetModuleLoaderFunc`, `JS_ResolveModule`, `JS_GetModuleNamespace`, `JS_EvalFunction`. |
| `crates/js-runtime/src/module_loader.rs` | UPDATE | Add the real `unsafe extern "C"` normalize/load callback pair (`module_normalize_fn`/`module_load_fn`) built on the existing `resolve_specifier`; still populates the existing `CACHES` map so a module fetched once is reused if imported again (real caching benefit kept, just synchronous now). |
| `crates/js-runtime/src/runtime.rs` | UPDATE | `Runtime::new` calls `sys::JS_SetModuleLoaderFunc(ptr, Some(module_loader::module_normalize_fn), Some(module_loader::module_load_fn), ptr::null_mut())`. |
| `crates/js-runtime/src/context/eval.rs` | UPDATE | New `Context::eval_module(code, filename) -> Result<String, EvalError>`: compiles via `JS_Eval(MODULE|COMPILE_ONLY)`, links via `JS_ResolveModule`, executes via `JS_EvalFunction`, returns the module namespace's stringified form (mirroring `eval`'s "coerce result to string" contract) — same exception-handling shape as `eval`. |
| `crates/profile/src/bin/profile_worker/page_source/scripts.rs` | UPDATE | `load_scripts` records each script's `type` attribute (already parses the tag's attributes) so `Page::load` can dispatch module vs classic. |
| `crates/profile/src/bin/profile_worker/page/load.rs` | UPDATE | A `type="module"` script now calls `ctx.eval_module(...)` instead of `ctx.eval(...)` — the actual "not treated as plain classic scripts anymore" fix `module_loader.rs`'s own doc calls out as remaining. |
| `crates/js-runtime/tests/es_modules_test.rs` | CREATE | Real linking/execution tests: static `import`/`export` across two real fetched module URLs, `import()` resolving a real namespace object, a `<script type="module">` page test at the `profile` level (or `js-runtime` level using a local test HTTP server, matching `net`'s existing test-server convention). |
| `spec/ROADMAP.md`, `spec/matrix/*.md` (JS runtime matrix, if it tracks modules) | UPDATE | Flip item 19 to `[~]` with the real scope cut once landed. |

## Tasks

### Task 1: `quickjs-sys` FFI bindings for the native module API
- **Action**: Add `JSModuleDef` (opaque, `#[repr(C)] struct JSModuleDef { _private: [u8; 0] }`, same shape as existing `JSRuntime`/`JSContext`), `JSModuleNormalizeFunc`/`JSModuleLoaderFunc` typedefs matching quickjs.h's exact signatures (`quickjs.h:1191-1203`), `JS_EVAL_TYPE_MODULE`/`JS_EVAL_FLAG_COMPILE_ONLY` constants (`1<<0`/`1<<5`), and `extern "C"` declarations for `JS_SetModuleLoaderFunc`, `JS_ResolveModule`, `JS_GetModuleNamespace`, `JS_EvalFunction`.
- **Mirror**: Existing `ffi.rs`/`types.rs` split and declaration style exactly — no new binding-generation machinery.
- **Validate**: `cargo build -p quickjs-sys` (pure FFI surface, nothing to unit-test at this layer — this crate has no tests of its own today, matching its existing convention).

### Task 2: Real module normalize/load callbacks (`js-runtime::module_loader`)
- **Action**: `module_normalize_fn(ctx, base_name, name, opaque) -> *mut c_char` calls `resolve_specifier`, allocates the result via `js_strdup`/`JS_MallocZ`-backed C string QuickJS expects to `js_free` itself (per `JSModuleNormalizeFunc`'s own contract in `quickjs.h`). `module_load_fn(ctx, module_name, opaque) -> *mut JSModuleDef` synchronously fetches via `net::request` (reusing/populating the existing `CACHES` map keyed by resolved URL so a module imported twice in one graph is fetched once), compiles via `JS_Eval(ctx, src, len, module_name, JS_EVAL_TYPE_MODULE | JS_EVAL_FLAG_COMPILE_ONLY)`, and returns the module pointer (`result.u.ptr as *mut JSModuleDef`) or `NULL` after throwing a real `JS_ThrowReferenceError` on fetch/compile failure (matching quickjs-libc.c's own reference `js_module_load`).
- **Mirror**: `quickjs-libc.c`'s `js_module_load` (read source, `JS_Eval` with the module+compile-only flags, extract the pointer) — the one real reference implementation to follow, adapted from its `load_file` callback to a blocking `net::request` call.
- **Validate**: `cargo build -p js-runtime` (no test surface yet at this layer alone — proven end-to-end in Task 4).

### Task 3: `Runtime::new` registers the loader; `Context::eval_module`
- **Action**: `Runtime::new` calls `JS_SetModuleLoaderFunc` once, right after `JS_NewRuntime`. `Context::eval_module` does the compile → `JS_ResolveModule` (links the whole static-import graph, invoking Task 2's load callback recursively for each) → `JS_EvalFunction` (executes top-level module code) sequence, with the same exception-to-`EvalError` handling `eval`/`eval_raw` already use — reusing `take_exception_text`'s shape rather than a third copy.
- **Mirror**: `Context::eval`'s existing structure line-for-line, just with the extra link/execute steps QuickJS's module contract requires between compile and "read the result."
- **Validate**: `cargo build -p js-runtime` + a first smoke test in Task 4's new file (`eval_module` on a module with no imports at all must behave identically to `eval` for a plain top-level script).

### Task 4: Wire `<script type="module">` + real tests
- **Action**: `load_scripts` (`page_source/scripts.rs`) captures each `<script>`'s `type` attribute alongside its source; `Page::load` (`page/load.rs`) calls `ctx.eval_module(script, url)` instead of `ctx.eval(script, ...)` when `type == "module"`. New `es_modules_test.rs`: (a) a module with a named export, imported by another module via real static `import { x } from './a.js'`, asserting the importer's own top-level code observes the real exported value; (b) `import('./a.js').then(ns => ...)` from a plain classic script resolving a real namespace object with the correct export; (c) a `<script type="module">` page test (via `profile`'s existing HTTP-server-per-test convention) proving a real module script executes and its side effect (e.g. a DOM mutation) is observable in rendered output; (d) a circular `import` pair (A imports B, B imports A) doesn't hang or crash — QuickJS's own module linker handles the cycle, this just proves the host wiring doesn't break it.
- **Mirror**: `crates/profile/tests/script_execution_test.rs`'s existing page-script test shape for (c); `crates/js-runtime/tests/*.rs`'s plain `Context::eval` assertion style for (a)/(b)/(d).
- **Validate**: `cargo test -p js-runtime -p profile --release -- --test-threads=1`.

## Validation

```bash
cargo build -p quickjs-sys -p js-runtime -p profile
cargo test -p js-runtime -p profile --release -- --test-threads=1
cargo build --workspace --exclude shell
cargo test --workspace --exclude shell --exclude automation --release -- --test-threads=1
cargo fmt --all
```

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Synchronous (blocking) module fetch inside the loader callback stalls the calling thread (a page with several imports fetches serially, on whichever thread evaluates the module) | High (accepted) | Documented scope cut — Stage 4's async cache still gets consulted/populated so a URL fetched once isn't re-fetched, but the *first* fetch of any given module blocks. A real non-blocking version would need QuickJS-ng's async module support (if any) or a different linking strategy — out of scope for this pass. |
| `JSModuleNormalizeFunc`'s returned C string ownership contract (QuickJS calls `js_free(ctx, ...)` on it, not plain `libc::free`) mismatched with Rust's allocator | Medium | Must allocate via `sys::js_strdup`-equivalent (a `JS_NewStringLen`-adjacent malloc helper) or match quickjs.h's own documented convention exactly — verify against `quickjs.c`'s own default normalizer implementation before trusting `CString::into_raw` + plain `free` would be safe (it would not, if QuickJS's own allocator differs from the system one). |
| Circular imports could infinite-loop or double-execute if the host wiring re-invokes the loader for an already-registered module | Low | QuickJS-ng's own `JS_ResolveModule`/module table handles cycles and re-entrant resolution internally (same graph algorithm every real JS engine implements) — Task 4(d)'s test exists specifically to catch a host-side wiring bug here, not to test QuickJS's own linker. |
| No import maps (`<script type="importmap">`) — bare specifiers (`import x from "lodash"`) still can't resolve, only relative/absolute URLs | High (accepted) | Documented scope cut, matching `module_loader.rs`'s own existing doc ("a bare specifier with no import map... same as a real browser without one configured"). |
| `import.meta` (`import.meta.url`) not implemented — `js_module_set_import_meta` is an internal quickjs-libc.c helper, not part of the public header | High (accepted) | Documented scope cut; no code in this workspace currently depends on `import.meta`. |
| `JS_EvalFunction`'s return value for a module (vs. a classic script) may not stringify the way `eval`'s existing contract expects (e.g. `undefined` always, or a pending top-level-await promise) | Medium | Task 3's own first smoke test checks this directly before Task 4 builds on it; if the return value isn't useful, `eval_module`'s documented contract becomes "runs for side effects, returns `Ok(String)` only to signal success" rather than a meaningful completion value — still consistent with how `<script type="module">` is used in practice (its return value is never observed by a real page either). |

## Acceptance
- [ ] Task 1-4 complete, each independently testable
- [ ] `es_modules_test.rs` proves real static import/export bindings, `import()` resolving a real namespace, and a `<script type="module">` page actually executing (not silently falling back to classic-script behavior)
- [ ] A circular import pair doesn't hang/crash
- [ ] Full workspace build + test green
- [ ] `spec/ROADMAP.md` item 19 updated with the real scope cut (no import maps, no `import.meta`, synchronous/blocking module fetch)
