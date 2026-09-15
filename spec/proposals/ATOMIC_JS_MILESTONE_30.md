# AtomicJS — Milestone 30: Modular execution core

Status: active

## Objective

Complete the execution-core refactor so each AtomicJS source module stays
below 350 lines without changing the supported language subset or tiering
semantics.

## Scope

- Split compiler expression lowering from orchestration, statement lowering,
  and static analysis.
- Split VM execution, object/property handling, feedback, and Tier-1
  admission into cohesive modules.
- Preserve bytecode layout, closure capture behavior, direct-call admission,
  feedback counters, and Tier-1 fallback behavior.

## modular-execution-core

The VM and compiler modules belong to this milestone while their
responsibilities are being separated.

## Gates

- `cargo test --manifest-path crates/atomicjs/Cargo.toml`
- `cargo clippy --manifest-path crates/atomicjs/Cargo.toml --lib -- -D warnings`
- Differential cases continue to match QuickJS-ng.
- Verify every Rust source implementation module is below 350 lines.

## Non-goals

- New JavaScript syntax, JIT tiers, allocator redesign, or public API changes.
