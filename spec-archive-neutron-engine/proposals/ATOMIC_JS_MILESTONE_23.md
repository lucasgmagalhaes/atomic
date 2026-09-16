# AtomicJS — Milestone 23: Tier-1 static numeric call graphs

## Status

Shipped (2026-09-15).

## Objective

Extend Tier 1 from an isolated numeric function to a statically resolved,
closure-free graph of numeric function declarations. The unit of admission is
the graph, not an individual caller: installing `driver` must never dispatch a
Tier-1 `helper` that escaped policy, budget, or lifecycle invalidation.

## Supported shape

- Named declarations that are lexically known before a direct call site.
- Exact numeric arguments and returns.
- Acyclic, closure-free call graphs whose every member independently lowers to
  the existing numeric Tier-1 subset.
- Calls may be nested inside numeric arithmetic and loops.

## Explicit exclusions

- Recursion and mutual recursion, closures/upvalues, function values,
  reassignment, dynamic property calls, objects, native calls, exceptions, and
  JavaScript coercion.
- Native JIT, executable memory, background compilation, or product/runtime
  integration.

## Design contract

### Direct-call bytecode

The compiler emits a new direct-call instruction only after resolving a
declaration to a stable module function index. It must preserve generic
`Call`/`CallLocal0` for every other callee. The direct instruction carries the
target index and arity, so it neither materializes a closure nor relies on a
mutable local slot.

### Group candidates and admission

At compile time, derive each candidate's transitive direct-call closure and
its aggregate code-size estimate. Reject the candidate if a member is
unsupported, captures an upvalue, or creates a cycle.

The tiering controller admits the root only when its whole closure fits the
hard budget. Installation is atomic for that closure, and pause invalidation
removes all installed members. No candidate may borrow an unadmitted function
from the global candidate cache.

### Execution and fallback

Tier-1 direct calls pass numeric arguments directly to their installed Tier-1
target. An unexpected call shape, missing target, or guard failure returns to
Tier 0 before either caller or callee mutates observable state. Tier-1 loop
feedback remains attached to the admitted root; helper feedback is collected
during Tier-0 warm-up before the graph is installed.

## Validation

- Bytecode tests prove only declared, static calls become direct calls.
- Tier-1 tests cover one helper, a three-node chain, rejected cyclic graphs,
  and every fallback boundary.
- Tiered-program tests prove group admission, aggregate budget rejection,
  pause/resume invalidation, and no partial installation.
- Differential fixtures compare Tier 0, admitted Tier 1, and QuickJS-ng for
  numeric helper chains and loops.
- The cross-engine Criterion harness measures the canonical helper-loop
  fixture with the same setup/hot-call boundary as Milestone 22.

## Acceptance criteria

1. An admitted root dispatches every member of its supported static graph in
   Tier 1, and no unadmitted member does.
2. Aggregate graph bytes, not root bytes, enforce the code budget.
3. Tier 0, Tier 1, and QuickJS-ng agree on every supported fixture.
4. Pause removes the entire installed graph and resume requires fresh warm-up.
5. `cargo test --manifest-path crates/atomicjs/Cargo.toml`, library Clippy,
   and the cross-engine benchmark pass.
