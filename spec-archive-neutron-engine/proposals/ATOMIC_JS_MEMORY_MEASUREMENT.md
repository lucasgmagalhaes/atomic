# AtomicJS — Memory measurement protocol

Every performance milestone must report both latency and memory.

## Required measurements

- Criterion latency for `compile_and_run` and `run_compiled`.
- Peak resident set size (RSS) for the equivalent one-shot AtomicJS and
  QuickJS workloads.
- A repeated-run RSS sample when a change affects VM frame, cache, closure,
  object, or allocation behavior.

## Commands

Use the existing comparison binaries, which print `maxrss_bytes` alongside
elapsed time and result:

```sh
cargo run --release --manifest-path crates/atomicjs/Cargo.toml --example atomicjs-run -- hot_loop
cargo run --release --manifest-path crates/js-runtime/Cargo.toml --example quickjs-run -- hot_loop
```

Run each command repeatedly on the same host, record the distribution rather
than a single sample, and state the platform because `ru_maxrss` units are
platform-dependent. Do not compare RSS from a debug binary against a release
binary, nor use RSS alone to claim steady-state heap usage.

## Reporting rule

Each new performance proposal must include a table with workload, release
mode, AtomicJS peak RSS, QuickJS peak RSS (when the workload is shared), and
the measurement command. If a change has no meaningful memory impact, say so
explicitly and retain the measurement evidence.
