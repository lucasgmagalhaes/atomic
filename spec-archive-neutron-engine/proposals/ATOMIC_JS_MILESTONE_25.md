# AtomicJS — Milestone 25: Tier-1 constant-left numeric leaf inlining

## Status

Shipped (2026-09-15).

## Objective

Eliminate the remaining direct-call frame for a closure-free Tier-1 numeric
leaf whose constant is the left operand, without broadening AtomicJS's
language semantics or its Tier-0 fallback.

## Supported shape

- A statically resolved direct call with one numeric argument.
- A closure-free leaf exactly equivalent to `return constant <op> arg`.
- Numeric `+`, `-`, `*`, `/`, and `%` operations.

## Non-goals

- General inlining, more than one argument, locals, branches, loops,
  recursion, closures, objects, coercion, or changes to Tier-0 dispatch.

## Validation

- Tier-1 and Tier-0 produce the same result for every supported operation.
- The differential suite compares the Tier-1 result with QuickJS-ng.
- Tiering admission and pause/resume lifecycle gates remain unchanged.

## Measurement methodology

This is a dispatch-elision extension, not a new performance claim. Any latency
claim must use Milestone 22's parse-excluded, release-mode cross-engine
protocol; this milestone records only correctness and eligibility evidence.
