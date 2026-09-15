# AtomicJS — Milestone 21: Tier-1 lifecycle integrity

## Status

Planned.

## Objective

Make Tier 1's lifecycle truthful and semantically conservative before expanding
its numeric subset or attempting native code generation. A function reported as
admitted must have an executable Tier-1 representation; every unsupported
call shape must remain wholly in Tier 0; and a pause must revoke compiled work
even if the host resumes before the next program execution.

This is the next increment after Milestone 20's differential gate. Milestone
20 proves that two promoted numeric programs agree with Tier 0 and QuickJS-ng.
It does not yet prove that Tier 1's *admission state* reflects installation,
that every argument shape preserves fallback semantics, or that the pause
transition cannot retain a stale admission.

## Why this comes before more Tier-1 coverage

The executable tier is deliberately optional. Its safety contract is therefore
stronger than its performance contract: a missed promotion is acceptable, but
a false promotion or a stale compiled entry is not.

Three current implementation details need an explicit contract and coverage:

1. `TieredProgram::compile` records `0` estimated bytes when
   `TierOneFunction::compile` rejects a function. The current policy can still
   mark that function `Eligible`, because a zero-byte admission fits every
   budget, even though no Tier-1 function can be installed.
2. `TierOneFunction::run` initializes all locals to `0.0` and copies only the
   supplied arguments. A call supplying fewer numeric arguments than the
   declared parameter count can therefore run with invented zero values rather
   than immediately falling back to Tier 0.
3. `TieredProgram::set_host_state(Paused)` clears installed functions, while
   `TieringController` clears its admission accounting only during a later
   `observe`. A `Paused → Running` transition before that observation can
   preserve an earlier admission and recompile immediately, contrary to the
   pause-invalidates-selection contract in Milestone 19.

## Scope

### In scope

- Separate *candidate support* from *policy eligibility* and *installed Tier-1
  code*.
- Reject non-exact argument arity in the Tier-1 executor before it allocates
  locals or executes any instruction.
- Invalidate Tier-1 admission and installed representations at the pause
  transition, not only when a later run observes the paused state.
- Expose enough per-run observability for tests and the RSS probe to distinguish
  supported, eligible, installed, executed, and fallback functions.
- Add deterministic Tier 0 / Tier 1 / QuickJS-ng differential cases covering
  lifecycle transitions and numeric function shapes.
- Re-run the steady-state Criterion benchmark and repeat the RSS probe; report
  results without claiming a win unless the confidence interval supports one.

### Explicitly out of scope

- New JavaScript syntax, object/property support in Tier 1, closures, native
  calls, type feedback, guards that speculate on objects, or deoptimization.
- Background compilation, executable memory, Cranelift, threads, or any
  integration with `js-runtime`, `profile-worker`, or product pause handling.
- Eviction under a soft budget. The existing hard per-program budget remains
  the policy boundary.
- Changing Tier 0 semantics for missing arguments. This milestone only ensures
  Tier 1 cannot invent a different result.

## Design contract

### 1. Eligibility requires support

During `TieredProgram::compile`, lower each function at most once into an
internal entry that retains both its support state and its exact estimated heap
cost. Unsupported functions have no candidate and are always reported as
`Interpret`; they must never reserve budget or be marked admitted.

Do not use `estimated_bytes == 0` as a support sentinel. A future valid
zero-byte representation would make that encoding ambiguous. Use an explicit
`Option<TierOneFunction>` (or a dedicated candidate struct) instead.

The controller's input should include support information, or the caller must
filter unsupported functions before invoking the policy. The resulting public
run record must make the distinction clear:

| State | Meaning |
| --- | --- |
| `Interpret` | Not supported, disabled, paused, below threshold, or refused by budget. |
| `Eligible` | Supported and admitted by policy; installation is attempted. |
| `Installed` | A Tier-1 representation exists and may execute on a later call. |
| `Executed` | This run actually dispatched through Tier 1. |
| `Fallback` | A supported installed entry rejected this call shape before execution, so Tier 0 ran it. |

`TierDecision` may remain the compatibility-facing admission result if a
separate `TierOneRunStats` provides the finer states. Avoid breaking the small
public API unless the tests demonstrate ambiguity that cannot be resolved by an
additive field.

### 2. Exact numeric call shape

Store `param_count` in `TierOneFunction`. `run(args)` must return `None`
unless `args.len() == param_count` and every argument is `Value::Number`.
Perform this validation before constructing locals or touching the operand
stack. A rejected shape increments fallback observability but must not count as
a specialized execution or alter loop feedback beyond the interpreter's own
execution.

Local declarations are still initialized by Tier 1's numeric subset as needed
by its lowered bytecode. Parameter slots are distinct from those local
declarations; do not use a missing parameter to infer a numeric zero.

### 3. Pause is a transition, not only a run-time condition

Add an explicit controller invalidation operation, called whenever host state
changes from `Running` to `Paused`. It clears admitted flags and byte
accounting synchronously. `TieredProgram` clears installed Tier-1 functions in
the same transition.

Repeated `Paused` calls are idempotent. `Paused → Running` does not restore a
previous selection: functions must gather fresh feedback and cross the policy
threshold again. The next run while paused remains interpreter-only and leaves
no compiled representation installed.

### 4. Budget accounting

Reserve bytes only after a supported candidate becomes eligible. Installation
must use the exact candidate whose size was admitted; do not lower a second
time with potentially divergent accounting. If allocation/lowering cannot
produce an entry, release no new JS-visible state and report `Fallback` or
`Interpret` according to the final contract. The hard cap remains monotonic
within one running epoch and resets on pause.

## Implementation plan

### Task 1 — Model candidates and run observability

Files: `crates/atomicjs/src/tier_one.rs`, `crates/atomicjs/src/vm.rs`,
`crates/atomicjs/src/lib.rs` if an additive public type is needed.

- Add `param_count` to `TierOneFunction` and preserve the lowering result as a
  candidate rather than discarding it after computing bytes.
- Define a minimal per-run stats structure: supported candidates, installations,
  Tier-1 dispatches, and fallbacks. Keep it numeric/indexed by function to
  match existing `decisions` indexing.
- Update `TieredRun` additively, document whether each field records the
  current run or persistent state, and retain `tier_one_calls` for compatibility.
- Ensure unsupported bytecode never produces `Eligible` or reserves budget.

### Task 2 — Enforce non-mutating call-shape fallback

Files: `crates/atomicjs/src/tier_one.rs`, `crates/atomicjs/src/vm.rs`.

- Validate exact arity and numeric argument tags at the beginning of
  `TierOneFunction::run`.
- Route `None` to the existing interpreter call path without counting a Tier-1
  execution.
- Keep the fallback boundary before any Tier-1 local/stack mutation. Document
  why this is safe even if future lowering adds guards.

### Task 3 — Make pause invalidation synchronous

Files: `crates/atomicjs/src/tiering.rs`, `crates/atomicjs/src/vm.rs`.

- Add a controller method dedicated to admission invalidation.
- Invoke it only for `Running → Paused`; preserve idempotence for repeated state
  updates.
- Remove duplicated, delayed clearing from `observe` only if the resulting
  paused-run behavior is still deterministic. Otherwise keep it as a defensive
  assertion-level backstop, not the primary mechanism.

### Task 4 — Correctness and differential coverage

Files: `crates/atomicjs/tests/tiering_test.rs`,
`crates/atomicjs/tests/tiered_program_test.rs`,
`crates/atomicjs/tests/tier_one_test.rs`,
`crates/atomicjs/tests/differential_test.rs`.

- Unsupported property access reaches the call threshold but stays `Interpret`,
  consumes zero budget, and reports no installation.
- A numeric function with too few arguments warms up but executes through Tier
  0 on the accelerated run; its result/error behavior must equal Tier 0.
- A nonnumeric argument rejects the specialized call before execution and
  preserves Tier 0 behavior.
- `Running → Paused → Running` before any `run` removes admission. Confirm the
  function needs a fresh threshold crossing before Tier 1 dispatches again.
- A paused run neither dispatches Tier 1 nor installs candidates; repeated pause
  calls remain stable.
- Two supported functions under a one-function byte budget admit only the first
  eligible candidate; an unsupported function cannot consume the available slot.
- Extend the QuickJS differential gate with every numeric operator and branch
  shape the Tier-1 lowering accepts. Each case must assert that Tier 1 actually
  executed when its call shape is valid.

### Task 5 — Measurement and documentation

Files: `crates/atomicjs/benches/atomicjs_bench.rs`,
`crates/atomicjs/examples/tiered-run.rs`, this milestone document.

- Add benchmark labels only if they distinguish an existing metric from new
  lifecycle accounting. Do not add a synthetic workload solely to improve a
  chart.
- Run Criterion with an immediate pre-change baseline and a post-change
  comparison. Record median, 95% interval, sample count, and whether the
  result is significant.
- Run the existing `tiered-run` probe at a fixed repetition count at least five
  times; report maximum RSS range and installed/dispatch/fallback counts.
- Update `## Results` only after correctness gates and measurements complete.

## Acceptance criteria

1. No unsupported function is reported as `Eligible` or `Installed` under any
   threshold or budget.
2. Tier 1 dispatches only with exact parameter arity and numeric arguments.
3. Every Tier-1 fallback has the same result or failure behavior as Tier 0, and
   executes no partial Tier-1 side effect before falling back.
4. A pause revokes both admitted bytes and installed functions immediately;
   resume does not restore them implicitly.
5. Tier 0, promoted Tier 1, and QuickJS-ng agree for every supported
   differential fixture.
6. `cargo test --manifest-path crates/atomicjs/Cargo.toml` passes.
7. `cargo clippy --manifest-path crates/atomicjs/Cargo.toml --lib -- -D warnings`
   passes. Test-target lint failures that predate this work must be documented
   separately rather than hidden by this milestone.
8. The benchmark and RSS records are reproducible, include their commands, and
   make no performance claim when results are statistically inconclusive.

## Result template

Populate after implementation:

- Correctness: `<tests and differential cases>`
- Lifecycle: `<pause/resume and budget observations>`
- Criterion: `<median, interval, significance>`
- RSS: `<runs, range, installed bytes/counts>`
- Scope confirmation: `<no JIT/native code/runtime integration>`

## Risks and decision rules

- If exact fallback cannot preserve Tier 0 behavior without executing Tier 1
  partially, narrow the accepted lowering subset instead of adding deoptimization.
- If lifecycle counters materially erase the steady-state Tier-1 win, retain
  correctness but do not make Tier 1 the default for any workload.
- If RSS rises materially relative to Milestone 20's probe, reduce the hard
  budget or stop the expansion; a smaller resident footprint is the project
  thesis.
- Do not begin Cranelift, background compilation, or browser integration from
  this milestone. Those require a separate scope decision and real-profile
  evidence.
