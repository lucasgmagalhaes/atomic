# AtomicJS — Milestone 8: Differential correctness gate

## Status

Completed.

## Objective

Protect the deliberately supported AtomicJS subset from semantic regressions by
checking its observable result against the repository's QuickJS-ng runtime.

## Delivered

- A development-only `js-runtime` dependency in `crates/atomicjs`.
- Differential tests that evaluate the same source in AtomicJS and QuickJS-ng
  and require identical stringified results.
- Four stable coverage programs for objects, closures, control flow/native
  math, and cross-function recursion.
- Five generated arithmetic/control-flow variants with distinct operands.

The oracle is test-only: AtomicJS's public API and production dependency graph
remain unchanged. No application integration is introduced by this milestone.

## Validation contract

The gate currently covers nine programs within the supported subset. It is an
exact-result oracle, not a syntax-compatibility claim for all JavaScript. A new
language feature must add a focused differential case before it is considered
stable.

## Explicit non-goals

- Fuzzing arbitrary JavaScript or testing unsupported grammar.
- Replacing AtomicJS unit tests, parser tests, or benchmark measurements.
- Adding a production dependency from any crate to AtomicJS.
- Making performance assertions from correctness tests.

## Follow-up

The next correctness increment is a deterministic, seeded generator with
failure shrinking for the supported expression and statement grammar. Keep it
bounded so the fast unit-test path remains practical.
