# AtomicJS — Milestone 20: Tier-1 differential execution gate

## Status

Completed.

## Objective

Make the executable Tier 1 safe to evolve by exercising an actually promoted
numeric function and comparing its result with both AtomicJS Tier 0 and the
QuickJS-ng oracle.

## Delivered

- Added an eager tiering harness to the existing differential test suite.
- Verified that warmup preserves the Tier 0 result before a Tier-1 function is
  installed.
- Required the second execution to dispatch through Tier 1, so the test cannot
  pass by comparing only interpreter fallbacks.
- Covered the supported numeric loop and the arithmetic/comparison subset.

## Validation

`cargo test --manifest-path crates/atomicjs/Cargo.toml --test differential_test`
passes with the Tier 1 path exercised. The test compares its result to both
the same source evaluated in Tier 0 and QuickJS-ng.

## Scope boundary

This adds no new language support, code generation, public runtime API, or
application integration. Unsupported bytecode and nonnumeric inputs continue
to use Tier 0.
