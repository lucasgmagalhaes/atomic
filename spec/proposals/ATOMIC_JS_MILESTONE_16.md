# AtomicJS — Milestone 16: Direct local numeric operations

## Status

Completed.

## Objective

Reduce bytecode dispatch for the repeated numeric-local expression shapes in
the representative idle-game tick.

## Delivered

- Added direct bytecodes for numeric operations between two known locals and
  between a known local and a numeric constant.
- Covered addition, subtraction, multiplication, division, modulo, and less
  than while leaving non-local and effectful expressions on the generic path.
- Added compiler coverage for both direct forms; the existing exact hot-loop
  result and QuickJS differential suite validate execution semantics.

## Result

The pre-change working tree was benchmarked immediately before the current
tree with 40 Criterion samples and 0.75 s measurement:

| Workload | Current median | Change | 95% interval | Result |
| --- | ---: | ---: | --- | --- |
| `hot_loop/compile_and_run` | 28.714 ms | -20.88% | -25.87% to -15.93% | improved, p < 0.05 |
| `hot_loop/run_compiled` | 33.241 ms | +5.50% | -1.04% to +13.60% | inconclusive |

The isolated compile-and-run result is statistically significant. The compiled
path varied on the shared host and is explicitly not claimed as a regression
or win; retain the Criterion command for a quiet-host confirmation.

`cargo test --manifest-path crates/atomicjs/Cargo.toml` passes all 73 tests,
and `cargo clippy --manifest-path crates/atomicjs/Cargo.toml --lib -- -D
warnings` passes for AtomicJS.

## Scope boundary

This is baseline-interpreter lowering only. It adds neither a JIT nor type
feedback, public APIs, application integration, or a new language feature.
