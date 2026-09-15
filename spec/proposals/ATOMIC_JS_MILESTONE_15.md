# AtomicJS — Milestone 15: Direct zero-argument call accumulation

## Status

Completed.

## Objective

Reduce dispatch and operand-stack traffic in the million-iteration closure
workload's `total += counter()` pattern without changing call semantics.

## Delivered

- Added `AddLocalCallLocal0`, emitted for discarded local `+=` expressions
  with a known zero-argument local-function call on the right.
- Reused the direct local call path instead of materializing the function and
  result on the operand stack before the addition.
- Kept an ownership-safe fallback for captured callable locals.
- Added a semantic regression test proving that the left value is read before
  a call that mutates the captured accumulator.

## Result

To avoid the shared-host variation observed in historical Criterion data, the
pre-change working tree was benchmarked immediately before the current tree,
with the same Criterion configuration (40 samples, 0.75 s measurement).

| Workload | Current median | Change | 95% interval |
| --- | ---: | ---: | --- |
| `closures/compile_and_run` | 37.223 ms | -14.33% | -16.73% to -11.94% |
| `closures/run_compiled` | 38.488 ms | -8.96% | -12.87% to -4.10% |

Both measurements reported p < 0.05.

`cargo test --manifest-path crates/atomicjs/Cargo.toml` passes all 72 tests,
and `cargo clippy --manifest-path crates/atomicjs/Cargo.toml --lib -- -D
warnings` passes for AtomicJS.

## Scope boundary

The specialization applies only to known local zero-argument calls. Generic
calls, calls with arguments, and all other compound assignments retain their
existing bytecode paths; there is no application integration or public API
change.
