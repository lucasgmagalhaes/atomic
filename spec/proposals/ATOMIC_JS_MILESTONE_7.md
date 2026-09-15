# ATOMIC_JS_MILESTONE_7.md — Isolated Compiled-Program API

## Status

Completed 2026-09-15. AtomicJS remains an isolated crate: this milestone adds no
dependency from `js-runtime`, `profile`, `workers`, or any other production crate.

## Question

Can AtomicJS expose a host-ready boundary for compile-once/run-many use while preserving
the fresh execution state expected of separate script evaluations?

## Scope

- Add `CompiledProgram::compile(&str)` to own immutable AtomicJS bytecode.
- Add `CompiledProgram::run()` and `run_with_feedback()`.
- Refactor `run_source` and `run_source_with_feedback` through that same boundary.
- Keep each run isolated: top-level locals, object literals, closures, VM pools, and
  feedback are new per invocation.
- Do not add an integration adapter, feature flag, root-workspace dependency, or modify
  QuickJS paths.

## Results

Two regression tests prove the contract: a compiled closure program returns `3` on two
successive runs rather than retaining closure state, and feedback is fresh and opt-in for
each execution. `cargo test --manifest-path crates/atomicjs/Cargo.toml` passed all 66
tests; formatting and strict library Clippy passed.

This is an architectural readiness improvement, not a cross-engine throughput claim.
The existing comparison runners continue to measure parse, compilation, and execution
together so their AtomicJS/QuickJS-ng comparison stays fair.

## Follow-up boundary

The next isolated milestone should add differential test generation for the supported
subset, not more execution shortcuts. A future host integration can consume
`CompiledProgram` only after a separate, explicit product decision and compatibility
scope.

---

[← `ATOMIC_JS_MILESTONE_6.md`](ATOMIC_JS_MILESTONE_6.md) · [← `ATOMIC_JS_SPIKE.md`](ATOMIC_JS_SPIKE.md)
