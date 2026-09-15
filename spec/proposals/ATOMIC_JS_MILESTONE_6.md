# ATOMIC_JS_MILESTONE_6.md — Direct Discarded Local Arithmetic Updates

## Status

Completed 2026-09-15. This is the final bounded interpreter fast-path milestone before
any integration decision. It targets the verified `sum` loop overhead without expanding
the language or changing observable expression values.

## Question

Can AtomicJS remove the repeated bytecode dispatch for the two discarded local updates
in `sum` — `total += i` and `i++` — while preserving normal expressions on the generic
path?

## Scope

- `AddLocalLocal { target, value }` replaces the discarded local-only `+=` sequence.
- `IncrementLocal` replaces a discarded local `++` sequence.
- The specialized opcodes are emitted only from `compile_discard_expr`; expressions that
  observe their result retain the existing load/add/store behavior.
- Upvalues, non-local right-hand sides, other operators, and all general arithmetic stay
  on the original bytecodes.

## Results

A compiler regression test pins both opcode selections; the exact `sum` completion test
continues to cover runtime semantics. `cargo test --manifest-path
crates/atomicjs/Cargo.toml` passed all 64 tests, and formatting plus strict library
Clippy passed.

Release measurements used `hyperfine --shell=none --warmup 10`:

| Session | Runs | AtomicJS `sum` | QuickJS-ng `sum` | Relative result |
| --- | ---: | ---: | ---: | --- |
| Before | 100 | 26.8 ms ± 2.0 | 23.2 ms ± 0.8 | QuickJS-ng 1.15× faster ± 0.09 |
| After | 100 | 21.7 ms ± 2.9 | 24.2 ms ± 2.9 | AtomicJS 1.12× faster ± 0.20 |
| After | 75 | 23.4 ms ± 6.2 | 23.2 ms ± 0.9 | AtomicJS 1.01× faster ± 0.27 |
| After | 100 | 21.7 ms ± 0.9 | 23.5 ms ± 1.3 | AtomicJS 1.08× faster ± 0.08 |

The two cleaner 100-run post-change sessions show AtomicJS ahead by 8–12%; the 75-run
session is affected by AtomicJS outliers and is reported as statistical parity. This is
the first reference workload with a reproducible AtomicJS lead, but it remains a
machine-local result rather than a portable claim.

## Integration gate

The current reference suite is within about 20% of QuickJS-ng on `props` and `closures`,
near parity or ahead on `sum`, and near parity on `hot_loop`, while one-shot RSS remains
about 1.6–1.7 MiB versus QuickJS-ng's 10.4–10.5 MiB. The next action changes scope:
either authorize an isolated feature-flag integration spike, or retain AtomicJS as an
experimental benchmark crate and remeasure on a dedicated host. Do not add a GC, JIT,
general shapes, or more opcode specializations without that decision.

---

[← `ATOMIC_JS_MILESTONE_5.md`](ATOMIC_JS_MILESTONE_5.md) · [← `ATOMIC_JS_SPIKE.md`](ATOMIC_JS_SPIKE.md)
