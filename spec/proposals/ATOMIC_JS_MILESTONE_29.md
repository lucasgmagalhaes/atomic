# AtomicJS — Milestone 29: Braced while loops

## Status

Shipped (2026-09-15).

## Objective

Add braced `while` loops to AtomicJS's supported statement subset by reusing
the existing bytecode jump and feedback semantics.

## Supported shape

- `while (condition) { statements }`.
- Conditions and bodies use the existing expression and statement subsets.
- Numeric loops remain eligible for the existing Tier-1 compiler.

## Non-goals

- `do...while`, labels, `break`, `continue`, lexical block scoping, and new
  truthiness or coercion rules.

## Validation and measurement

Lexer, parser, compiler, VM feedback, Tier-1 execution, and QuickJS-ng
differential gates cover the loop. This feature has no new performance claim.
