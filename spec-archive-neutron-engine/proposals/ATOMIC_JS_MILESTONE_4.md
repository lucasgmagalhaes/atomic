# ATOMIC_JS_MILESTONE_4.md — Compact Property Slots for Small Object Literals

## Status

Completed 2026-09-15. This is a bounded performance milestone following the repeated
`props` measurements after Milestone 3. It does not add general shapes, inline caches,
property assignment syntax, a GC, or a JIT.

## Question

Can AtomicJS remove the demonstrated property-read gap without widening the language or
adding a general object-model subsystem?

Before this work, `props` was stable at 42.7–43.3 ms while QuickJS-ng was 26.5–26.7 ms
on this machine: QuickJS-ng was about 1.6× faster. The known hot path used a
`HashMap<String, Value>` for every property read and materialized a local object's `Rc`
on the operand stack before reading it.

## Scope

- Store the properties of AtomicJS's small object literals as insertion-ordered inline
  `(String, Value)` slots. Reassigning an existing key replaces its value.
- Lower `local.property` and `captured.property` to dedicated bytecodes, avoiding an
  object stack round-trip and temporary `Rc` clone.
- Keep generic `GetProp` for non-identifier receiver expressions.
- Preserve `undefined` for reads from a missing property or non-object receiver.

## Results

Correctness coverage includes direct local reads, captured-object reads, missing
properties, and duplicate literal keys. `cargo test --manifest-path
crates/atomicjs/Cargo.toml` passed all 62 tests.

Release measurements used `hyperfine --shell=none --warmup 10`:

| Session | Runs | AtomicJS `props` | QuickJS-ng `props` | Relative result |
| --- | ---: | ---: | ---: | --- |
| Before | 75 | 43.3 ms ± 2.1 | 26.5 ms ± 0.7 | QuickJS-ng 1.63× faster ± 0.09 |
| Before | 75 | 43.0 ms ± 1.0 | 26.5 ms ± 0.7 | QuickJS-ng 1.63× faster ± 0.06 |
| After | 100 | 32.1 ms ± 0.9 | 27.9 ms ± 0.8 | QuickJS-ng 1.15× faster ± 0.05 |
| After | 75 | 33.2 ms ± 5.8 | 27.4 ms ± 1.5 | QuickJS-ng 1.21× faster ± 0.22 |
| After | 75 | 31.6 ms ± 1.8 | 27.0 ms ± 3.5 | QuickJS-ng 1.17× faster ± 0.17 |

The causal improvement is clear: AtomicJS's representative property loop improved by
about 26%, and the remaining gap is now roughly 15–21%, not 60%+. Outliers remain on
the shared host, so this does not claim a durable lead over QuickJS-ng.

## Follow-up boundary

Do not add full shapes or inline caches next by default. First remeasure all reference
workloads on a quiet host and profile the remaining `props` cost. A future milestone may
target the demonstrated remaining dispatch/slot-read cost, but it must keep exact
QuickJS differential tests and a separately stated scope.

---

[← `ATOMIC_JS_SPIKE.md`](ATOMIC_JS_SPIKE.md) · [← `ATOMIC_JS_MILESTONE_3.md`](ATOMIC_JS_MILESTONE_3.md)
