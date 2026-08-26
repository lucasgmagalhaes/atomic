# JS Runtime & Host — Capability Status

## 1. JavaScript runtime and host

**Done (75%)**

- [x] `Runtime`, `Context`, synchronous `eval` and exception conversion.
- [x] ES language execution through QuickJS-NG: classes, modules supported by the upstream runtime, promises, async functions, typed arrays, `Map`, `Set`, regex and standard built-ins.
- [x] Per-context DOM, storage and native-object state; per-runtime class registration.
- [x] Promise-job draining and host timer/event pumping.
- [x] Resource limits on selectors, DOM string inputs, event listeners, bubbling and nested dispatch.
- [x] `crypto.getRandomValues`.
- [x] `console` (`log`/`info`/`warn`/`error`/`debug`) with leveled message storage hosts can drain, and structured uncaught-error reporting (real exception text + a `window` `error` event).
- [x] Interrupt/time budget for untrusted script execution (`Context::set_time_budget`: quickjs's interrupt handler aborts a budget-exceeding `eval` with an "interrupted" InternalError; deadline re-stamped per call) and memory quotas per document (`Context::set_memory_limit`, quickjs's runtime-wide allocation cap — one runtime per context here, so per-document is exactly what it gives).

**Needed**

- [ ] ES-module loader/resolver, import maps, dynamic `import()` and module cache.
- [ ] Script loading modes: parser-blocking, `defer`, `async`, module scripts and CSP-aware loading.
- [ ] Source maps, debugger protocol and remaining structured error reporting (console + uncaught-error reporting are done — see section 1's Done list; `EvalError` now carries the real stringified exception instead of the old "script raised an exception" placeholder).
- [ ] Worker runtime: `Worker`, `SharedWorker`, `MessageChannel`, structured clone and transferable objects.

---

[← back to spec/INDEX.md](../INDEX.md)
