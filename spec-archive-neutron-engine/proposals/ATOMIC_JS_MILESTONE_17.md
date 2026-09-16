# AtomicJS — Milestone 17: Unboxed numeric local access

## Status

Completed.

## Objective

Let direct numeric bytecodes operate on local `f64` payloads without cloning
and replacing temporary `Value::Number` wrappers at every operation.

## Delivered

- Added inlined numeric read/write helpers for plain and captured local slots.
- Applied them to direct local arithmetic, direct property/call accumulation,
  direct numeric binary operations, and local increment.
- Preserved `Value` paths and invariant failures for values outside the
  numeric subset.

## Result

The immediately preceding working tree was benchmarked as a 40-sample,
0.75-second Criterion baseline before measuring this change:

| Workload | Current median | Change | 95% interval |
| --- | ---: | ---: | --- |
| `hot_loop/compile_and_run` | 24.437 ms | -14.67% | -17.26% to -12.14% |
| `hot_loop/run_compiled` | 23.330 ms | -20.67% | -22.62% to -18.88% |

Both comparisons reported p < 0.05.

`cargo test --manifest-path crates/atomicjs/Cargo.toml` passes all 73 tests,
including the QuickJS differential gate, and strict AtomicJS library Clippy
passes.

## Scope boundary

This is not a new tagged-value representation or general unboxing scheme.
It is a narrow interpreter implementation detail for already-proven numeric
bytecode paths, with no API or application integration change.
