# AtomicJS — Milestone 26: Tier-1 binary numeric leaf inlining

## Status

Shipped (2026-09-15).

## Objective

Eliminate direct-call overhead for a closure-free, two-argument numeric Tier-1
leaf while preserving operand order and Tier-0 as the semantic fallback.

## Supported shape

- A statically resolved direct call with exactly two numeric arguments.
- A closure-free leaf exactly equivalent to `return left <op> right`.
- Numeric `+`, `-`, `*`, `/`, and `%` operations.

## Non-goals

- Multi-expression or multi-statement leaves, general inlining, coercion,
  branches, closures, objects, recursion, and Tier-0 changes.

## Validation and measurement

Tier-0, Tier-1, and QuickJS-ng must agree on every supported operation. This
is dispatch elision only; any latency claim must use Milestone 22's
parse-excluded release benchmark protocol.
