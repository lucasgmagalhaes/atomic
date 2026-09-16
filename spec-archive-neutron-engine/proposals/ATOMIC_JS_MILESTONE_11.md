# AtomicJS — Milestone 11: Canonical benchmark scripts

## Status

Completed.

## Objective

Eliminate copied JavaScript fixture strings inside AtomicJS so a workload cannot
silently differ between the command-line runner, correctness tests, and
Criterion benchmarks.

## Delivered

- Seven source files in `crates/atomicjs/benchmarks/scripts/`:
  `sum`, `obj`, `closure`, `props`, `closures`, `hot_loop`, and `upgrade_cost`.
- `include_str!` consumers in the AtomicJS runner, test fixtures, and Criterion
  benchmark target.
- The JavaScript program text is now independently readable, editable, and
  runnable while remaining embedded at Rust compile time.

## Scope boundary

Only AtomicJS owns these canonical scripts. `js-runtime` remains unchanged;
this is not a runtime integration or a production dependency change.
