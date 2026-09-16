# AtomicJS — Milestone 14: Direct property accumulation

## Status

Completed.

## Objective

Remove avoidable interpreter dispatch and operand-stack traffic from the
representative idle-game accumulation pattern `total += player.damage`.

## Delivered

- Added `AddLocalProp`, emitted only for a discarded `+=` whose target and
  member receiver are known locals.
- Fused local-total load/store, direct member read, numeric addition, and the
  existing shape-guarded property cache into one VM instruction.
- Preserved the generic expression path for every other compound assignment.
- Added compiler coverage that pins the specialized selection and asserts it
  does not also emit a standalone property-read instruction.

## Result

The focused 60-sample Criterion run on 2026-09-15 compared with the saved
pre-property-cache baseline:

| Workload | Median | Change | 95% interval |
| --- | ---: | ---: | --- |
| `props/compile_and_run` | 17.161 ms | -22.45% | -24.89% to -19.73% |
| `props/run_compiled` | 17.088 ms | -22.46% | -24.80% to -19.93% |

Both comparisons reported p < 0.05. The optimization targets this measured
shape only; it is not presented as a general property-assignment or JIT
optimization.

`cargo test --manifest-path crates/atomicjs/Cargo.toml` passes all 70 tests,
and `cargo clippy --manifest-path crates/atomicjs/Cargo.toml --lib -- -D
warnings` passes for AtomicJS.

## Scope boundary

No public API, application integration, general object mutation syntax, or
pointer-identity guard was added. Cache state remains local to an execution.
