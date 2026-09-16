# AtomicJS — Milestone 9: Reproducible differential generation

## Status

Completed.

## Objective

Expand the QuickJS-ng differential gate beyond hand-written examples without
turning the normal AtomicJS test run into an unbounded fuzzer.

## Delivered

- A dependency-free, deterministic linear-congruential generator.
- Sixty-four generated programs from a recorded seed.
- Generated values exercise arithmetic, assignment, modulo, comparison, and
  conditional control flow while never generating division or modulo by zero.
- Failure minimization that repeatedly reduces individual operands while the
  AtomicJS/QuickJS-ng mismatch still reproduces.

## Failure contract

A failure reports the seed, case index, original source, minimized source, and
both observable results. The exact seed makes every generated corpus stable in
local development and CI.

## Scope boundary

The generator emits only the current AtomicJS subset. It is deliberately not a
general JavaScript fuzzer, and `js-runtime` remains a development-only oracle.
No production crate depends on AtomicJS or gains a runtime dependency from this
test suite.

## Validation

Run the isolated suite with:

```sh
cargo test --manifest-path crates/atomicjs/Cargo.toml
```
