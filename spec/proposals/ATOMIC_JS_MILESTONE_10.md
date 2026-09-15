# AtomicJS — Milestone 10: In-process performance regression gate

## Status

Completed.

## Objective

Make performance work measurable before adding interpreter complexity. The suite
separates AtomicJS's public compile-and-run path from the compile-once/run-many
boundary provided by `CompiledProgram`.

## Delivered

- Criterion benchmark target at `benches/atomicjs_bench.rs`.
- Four representative workloads: numeric loop, property loop, closure loop,
  and the idle-game hot loop.
- Two measurements per workload:
  - `compile_and_run`, through `run_source`;
  - `run_compiled`, through a single immutable `CompiledProgram` with fresh VM
    state on every iteration.

## Smoke results

On 2026-09-15, with Criterion's intentionally short 10-sample smoke setting,
the `run_compiled` medians were approximately:

| Workload | Time |
| --- | ---: |
| `sum` | 13.8 ms |
| `props` | 22.9 ms |
| `closures` | 39.2 ms |
| `hot_loop` | 30.4 ms |

These numbers establish a local regression baseline only. They are not a
cross-engine claim and should be remeasured using a longer Criterion run before
using them to select an optimization.

## Usage

```sh
cargo bench --manifest-path crates/atomicjs/Cargo.toml --bench atomicjs_bench
```

Use Criterion baselines around a proposed VM change:

```sh
cargo bench --manifest-path crates/atomicjs/Cargo.toml --bench atomicjs_bench -- --save-baseline before
# make and validate the change
cargo bench --manifest-path crates/atomicjs/Cargo.toml --bench atomicjs_bench -- --baseline before
```

## Scope boundary

This target depends only on AtomicJS and its test tooling. It does not add a
production integration, a JIT, a garbage collector, or a new language feature.
