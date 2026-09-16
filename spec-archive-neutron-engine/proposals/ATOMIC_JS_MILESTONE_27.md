# AtomicJS — Milestone 27: Tier-1 relational leaf inlining

## Status

Shipped (2026-09-15).

## Objective

Inline closure-free relational helpers into Tier-1 conditional callers while
retaining the existing boolean representation and interpreter fallback.

## Supported shape

- A direct Tier-1 helper returning `left < right`.
- One or two numeric parameters, with an optional numeric constant operand.
- The result may feed a Tier-1 conditional or be returned directly.

## Non-goals

- Equality, coercion, logical operators, branches inside the helper, general
  inlining, closures, objects, or Tier-0 changes.

## Validation and measurement

Tier-0, Tier-1, and QuickJS-ng must agree for relational helpers used by a
conditional. No latency claim is made; use Milestone 22's release benchmark
protocol before making one.
