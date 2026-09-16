# AtomicJS — Milestone 19: Tier-1 policy contract

## Objective

Introduce the first isolated tiering boundary without a JIT backend: decisions
must be deterministic, opt-in, bounded by a per-process memory budget, and
always preserve the interpreter as the semantic fallback.

## Contract

- Tier 0 is the existing bytecode interpreter.
- A function becomes eligible only after a configurable call or loop threshold
  while the host is `Running`.
- `Paused` hosts never promote and invalidate tier-1 selection lazily before
  the next call.
- A hard byte budget refuses new tier-1 installations; it never evicts or
  mutates executing code.
- Tier-1 failure, unsupported bytecode, guard failure, or disabled policy
  returns to Tier 0 without changing JS-visible state.

## First implementation boundary

Milestone 19 adds policy state and observability only. It deliberately does
not add a JIT, executable memory, a worker, or application integration. The
first executable tier may initially be an alternate prevalidated bytecode
entrypoint, but must demonstrate a benchmark win and report peak RSS before
becoming the default for any workload.

## Validation

- Unit tests for threshold, pause, budget, and fallback decisions.
- Existing AtomicJS/QuickJS differential gate for every executable-tier case.
- Criterion latency plus the RSS protocol in
  `ATOMIC_JS_MEMORY_MEASUREMENT.md`.
