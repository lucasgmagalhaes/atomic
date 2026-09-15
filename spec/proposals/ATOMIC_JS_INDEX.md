# AtomicJS — Proposal index

## Current entry points

- [Validation spike](ATOMIC_JS_SPIKE.md): the scope boundary, evidence model,
  and go/no-go decision record for the isolated interpreter.
- [Memory measurement protocol](ATOMIC_JS_MEMORY_MEASUREMENT.md): the canonical
  RSS methodology for AtomicJS and QuickJS-ng comparisons.
- [Tiering design notes](ATOMIC_JS_TIERING.md): historical policy rationale;
  Tier-1's executable contract is now implemented and verified by milestones
  19--21.
- [Milestone 21](ATOMIC_JS_MILESTONE_21.md): the current Tier-1 lifecycle,
  observability, correctness gates, and latest performance evidence.
- [Milestone 22](ATOMIC_JS_MILESTONE_22.md): the maintained AtomicJS Tier-1
  and QuickJS-ng hot-function comparison protocol.
- [Milestone 23](ATOMIC_JS_MILESTONE_23.md): the active design for admitting
  closure-free numeric call graphs as one Tier-1 unit.
- [Milestone 24](ATOMIC_JS_MILESTONE_24.md): the shipped narrow inlining path
  for numeric Tier-1 helper leaves.
- [Milestone 25](ATOMIC_JS_MILESTONE_25.md): constant-left numeric helper
  inlining for the same one-argument Tier-1 boundary.
- [Milestone 26](ATOMIC_JS_MILESTONE_26.md): two-argument numeric helper
  inlining at the same Tier-1 boundary.
- [Milestone 27](ATOMIC_JS_MILESTONE_27.md): relational helper inlining for
  Tier-1 conditionals.
- [Milestone 28](ATOMIC_JS_MILESTONE_28.md): optional braced `else` branches
  across the supported statement pipeline.

## Historical record

The milestone files are immutable decision and measurement records, not an
action queue. Read them only when a current proposal or a code path links to a
specific decision:

| Range | Theme |
| --- | --- |
| [2--7](ATOMIC_JS_MILESTONE_2.md) | bounded interpreter workloads and direct execution fast paths |
| [8--12](ATOMIC_JS_MILESTONE_8.md) | differential correctness, reproducibility, benchmarks, and the QuickJS-ng oracle |
| [13--18](ATOMIC_JS_MILESTONE_13.md) | property/local/call-path interpreter optimizations |
| [19--21](ATOMIC_JS_MILESTONE_19.md) | Tier-1 policy, differential dispatch, and lifecycle integrity |

## Superseded material

[The original architecture proposal](ATOMIC_JS_ARCHITECTURE.md) is retained as
a rejected design record. It must not be used as implementation authority.
The spike and the latest completed milestone supersede it for active work.

## Active work rule

New AtomicJS implementation proposals must state: the supported language
subset, a Tier 0 and QuickJS-ng correctness gate, measurement methodology, and
an explicit non-goal. Do not reopen old milestones merely to append a new task.
