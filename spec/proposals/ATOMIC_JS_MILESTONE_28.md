# AtomicJS — Milestone 28: Conditional else branch

## Status

Shipped (2026-09-15).

## Objective

Extend AtomicJS's supported statement subset with an optional braced `else`
branch, from lexical recognition through bytecode execution.

## Supported shape

- `if (condition) { statements } else { statements }`.
- Both branches use the existing statement subset and local-scope model.
- Omitted `else` preserves the existing `if` behavior.

## Non-goals

- `else if`, unbraced branches, truthiness changes, lexical block scoping,
  and Tier-1 compilation of newly complex conditional helpers.

## Validation and measurement

Lexer, parser, compiler, VM, and Tier-0/QuickJS-ng differential gates cover
both branch outcomes. This is a language feature, so it makes no performance
claim.
