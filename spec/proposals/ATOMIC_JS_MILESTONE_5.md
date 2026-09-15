# ATOMIC_JS_MILESTONE_5.md — Direct Calls for Zero-Argument Local Functions

## Status

Completed 2026-09-15. This bounded performance milestone follows the post-Milestone-4
workload sweep, where `closures` was the largest remaining reference-program gap.

## Question

Can AtomicJS improve its million-call closure loop without general call-site inline
caches, argument-specialization, or a new calling convention?

Before this work, the `counter()` call in `closures` compiled as `LoadLocal` then
`Call(0)`. Each iteration cloned the local `Rc<FunctionData>`, placed it on the operand
stack, and immediately removed it again before the callee ran.

## Scope

- Add `CallLocal0`, only for a zero-argument call whose callee is a known local
  identifier.
- Borrow `FunctionData` directly from a plain local slot, avoiding the local-load,
  operand-stack round trip, and `Rc` clone.
- Fall back to the existing ownership-safe value path for captured locals, because a
  RefCell borrow must not span a recursive call.
- Leave calls with arguments, upvalue calls, generic callees, and native calls unchanged.

## Results

The compiler test pins the specialized bytecode selection. The existing closure and
non-function-call tests cover completion and error semantics. `cargo test --manifest-path
crates/atomicjs/Cargo.toml` passed all 63 tests; formatting and strict library Clippy
also passed.

Release measurements used `hyperfine --shell=none --warmup 10`:

| Session | Runs | AtomicJS `closures` | QuickJS-ng `closures` | Relative result |
| --- | ---: | ---: | ---: | --- |
| Before | 75 | 60.7 ms ± 4.8 | 40.9 ms ± 8.7 | QuickJS-ng 1.48× faster ± 0.34 |
| After | 100 | 47.6 ms ± 1.2 | 39.0 ms ± 0.8 | QuickJS-ng 1.22× faster ± 0.04 |
| After | 75 | 47.3 ms ± 1.3 | 38.9 ms ± 0.9 | QuickJS-ng 1.22× faster ± 0.04 |
| After | 75 | 47.5 ms ± 1.0 | 41.3 ms ± 9.3 | QuickJS-ng 1.15× faster ± 0.26 |

AtomicJS improved about 22% on the targeted workload. The first two post-change sessions
are stable; the third has QuickJS-ng outliers and is reported without treating it as a
performance win. The remaining difference does not justify general call specialization
without a new representative workload and profile.

## Follow-up boundary

The next engineering step is not another micro-optimization by default. Run the complete
four-workload suite on a quiet host, preserve exact differential correctness gates, and
measure steady-state memory before deciding whether AtomicJS earns an integration spike.

---

[← `ATOMIC_JS_MILESTONE_4.md`](ATOMIC_JS_MILESTONE_4.md) · [← `ATOMIC_JS_SPIKE.md`](ATOMIC_JS_SPIKE.md)
