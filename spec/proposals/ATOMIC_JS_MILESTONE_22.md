# AtomicJS — Milestone 22: reproducible cross-engine hot benchmark

## Status

Completed 2026-09-15.

## Objective

Establish a maintained, in-process comparison between AtomicJS Tier 1 and the
repository's vendored QuickJS-ng for the same supported numeric workload. The
benchmark must distinguish runtime setup from hot execution and retain a
semantic check before timing either engine.

## Contract

- Both engines evaluate the canonical `benchmarks/scripts/sum.js` fixture once
  during setup.
- AtomicJS warms until its `sum` function dispatches Tier 1; QuickJS-ng looks
  up the installed global `sum` function.
- Criterion samples only `sum(1_000_000)`: AtomicJS uses its public
  compile-once/run-many `TieredProgram`, while QuickJS-ng calls the already
  installed function through `JS_Call`.
- Setup is intentionally outside the measurement loop. This is a hot-function
  comparison, not a parser, runtime-allocation, or process-start comparison.
- Before sampling, both paths assert the exact result `499999500000`; the
  existing differential suite remains the broader semantic gate.

## Non-goals

- This does not compare cold startup, full JavaScript coverage, JIT engines,
  browser integration, or resident memory. Those need separate fixtures and
  measurement protocols.
- It does not claim that the engines expose identical public APIs; the raw
  QuickJS-ng call exists only in the development benchmark target.

## Reproduction

```bash
cargo bench --manifest-path crates/atomicjs/Cargo.toml \
  --bench cross_engine_bench -- \
  --sample-size 20 --warm-up-time 1 --measurement-time 3
```

## Results

On 2026-09-15, the command above produced:

| Engine | Median | 95% interval |
| --- | ---: | --- |
| AtomicJS Tier 1 | 3.7761 ms | 3.7417--3.8117 ms |
| QuickJS-ng | 17.460 ms | 17.253--17.705 ms |

The intervals do not overlap in this run. This is evidence for the narrow,
numeric hot-function shape only; it is not a general throughput claim.
