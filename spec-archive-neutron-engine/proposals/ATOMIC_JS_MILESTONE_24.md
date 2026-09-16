# AtomicJS — Milestone 24: Tier-1 numeric leaf inlining

## Status

Shipped (2026-09-15).

## Objective

Remove direct-call frame overhead from the hottest Tier-1 helper shape without
expanding AtomicJS's language contract or weakening its interpreter fallback.

## Supported shape

- A statically resolved Tier-1 direct call with exactly one numeric argument.
- A closure-free leaf whose body is exactly `return arg <numeric-op> constant`.
- Numeric `+`, `-`, `*`, `/`, and `%` only.

## Non-goals

- General-purpose inlining, multi-argument calls, locals, branches, loops,
  recursion, closures, native calls, objects, or speculative type coercion.
- Any change to Tier 0 semantics or to the pause/resume invalidation contract.

## Design

Candidate preparation copies the Tier-1 candidate catalog, then replaces only
the proven leaf call with an `InlineUnaryConst` instruction in its caller.
Unsupported calls retain `CallDirect` and its guarded numeric fallback. The
inlined operation executes in the caller's Tier-1 stack, so it creates no
call frame, argument buffer, or helper-local buffer.

The transformation happens before group sizing and admission. Therefore the
installed caller remains lifecycle-bound to the existing Tier-1 policy, while
the eliminated edge consumes no helper code-budget reservation. Pausing still
clears every installed Tier-1 entry; a resumed program must warm up again.

## Validation

- Existing Tier-0 / Tier-1 / QuickJS-ng differential fixtures include the
  nested numeric helper chain and dynamic reassignment fallback.
- Tiered lifecycle tests cover atomic admission, budget rejection, and
  pause/resume invalidation.
- `cargo test --manifest-path crates/atomicjs/Cargo.toml` and library Clippy
  pass.
- The hot helper-loop Criterion comparison, with parse/setup excluded, measured
  AtomicJS at 17.24 ms median and QuickJS-ng at 61.91 ms median (10 samples,
  0.1 s warm-up, 0.2 s measurement; 2026-09-15). This is a directional
  microbenchmark, not a product-level claim.
