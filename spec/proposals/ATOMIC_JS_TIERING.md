# ATOMIC_JS_TIERING.md — The Interpret-vs-Compile Orchestrator (design notes)

## 0. Status

**Not in scope for the spike.** [`ATOMIC_JS_SPIKE.md`](ATOMIC_JS_SPIKE.md)'s §3 hard
scope limits explicitly exclude any JIT — the spike is interpreter-only. This document
exists purely to think through this one component ahead of time, because it's the piece
most likely to quietly undermine the product's own reason for existing if it gets
designed the way a general-purpose engine (V8, JSC) would design it. Nothing here is
scheduled work. It only matters if the spike's go/no-go (§9 there) turns green *and* a
later scope decision explicitly greenlights a baseline-JIT phase — at which point this
doc is the starting design, not the rejected proposal's.

**Related, independent prior art:**
[`.claude/plans/hybrid-interpreter-jit.plan.md`](../../.claude/plans/hybrid-interpreter-jit.plan.md)
(2026-09-13, speculative/not scheduled) designs a tiering orchestrator too, but for a
different architecture: it keeps QuickJS-ng itself as the tier-0 interpreter and bolts
an opt-in JIT tier onto QuickJS-ng's own bytecode dispatch, rather than assuming a
from-scratch interpreter (AtomicJS) is tier 0. That plan is also not triggered today —
its own trigger condition (a real profiled workload showing JS execution dominates
frame time, with JIT memory cost not regressing the per-profile footprint baseline) is
unmet. Read both before any real tiering work: they're mutually exclusive alternatives
(either QuickJS-ng or AtomicJS ends up as tier 0, not both), not complementary designs.

## 1. Why this component is the actual risk, not just a detail

The product's differentiator, per `spec/ROADMAP.md`'s 2026-08-26 product-scope decision
and `CLAUDE.md`'s positioning section, is memory/CPU footprint across **many concurrently
idle profiles** — not raw JS throughput. Each profile is its own OS process
(`crates/profile`/`profile-worker`, per `CLAUDE.md`'s Implementation status), so the
concrete cost being defended is *that one process's* resident memory while idle.

The rejected proposal's tiering model ([`ATOMIC_JS_ARCHITECTURE.md`](ATOMIC_JS_ARCHITECTURE.md)
§18–21, §24) is copied from the V8/JSC mental model: count calls/loop iterations per
function, compile it once it's "hot," keep the compiled code around indefinitely. That
model is designed for a browser with one (or a few) foreground tab(s) that stay hot for
a long session — it has no notion of "this hot loop belongs to a tab nobody is looking
at right now."

Applied naively here, it backfires: an idle game's tick loop *is* hot by iteration count
even while its pane sits in a background workspace. A naive orchestrator would compile
it, and the resulting machine code + `CodeObject` metadata + IC/profiler tables sit in
that process's resident memory for as long as the tab exists — for every such backgrounded
profile. That's reintroducing, per-profile, exactly the "compiled code costs memory"
problem this whole architecture exists to avoid by choosing QuickJS-ng (no JIT) in the
first place.

So this orchestrator's real job is **not** "decide when code is hot" — every engine
solves that, it's the easy part (§3). Its job is "decide when paying compiled-code
memory is actually worth it, given that this process might get backgrounded for hours
and nobody benefits from that memory being spent."

## 2. The real signal to plug into (already exists — don't invent a new one)

`spec/RULES.md`'s "Architecture-first" rule and `spec/architecture/overview.md`'s reuse
table both say: check for an existing primitive before building a new one. One already
exists here, and it's simpler than the rejected doc assumed:

- `apps/shell`'s `apply_visibility_throttling` ([workspaces.rs:53](../../apps/shell/src/app/workspaces.rs:53))
  calls `Profile::pause()`/`resume()` for every pane not in the active workspace.
- `Profile::pause()` sends `PAUSE` over the worker's stdin protocol
  ([scripting.rs:106-119](../../crates/profile/src/scripting.rs:106)); the worker's own
  doc there is explicit: paused means "no JS timer pumping, no re-render, no frame
  publish — genuinely idle."
- This is a **binary** state (paused / running), not the rejected doc's four-level
  `ExecutionPolicy` (`Foreground`/`VisibleBackground`/`Background`/`Frozen`). No pane
  today distinguishes "visible but not interacted with" from "actively focused," or
  "backgrounded" from "frozen/suspended." Don't build that finer granularity speculatively
  — YAGNI, per `spec/RULES.md`'s own translation of that principle. If a real need for
  more than two levels ever shows up, extend this signal then.

**Consequence for the orchestrator:** it should read the same paused/running state the
render loop already reads, not maintain a parallel notion of "is this profile hot right
now" independent of visibility. One source of truth for "is anyone looking at this,"
shared by the render loop and the tiering decision.

## 3. Inputs

- **Per-function feedback**, within this one process — call count, loop iteration
  count, cheap sampled counters (rejected doc §18's `FunctionFeedback` shape is fine
  unmodified: this part isn't the risky one).
- **This process's own paused/running state** (§2) — the product-specific signal a
  generic engine's tiering model doesn't have and this one must.
- **A per-process compiled-code footprint cap** — bytes of executable memory +
  `CodeObject` metadata, kept low. This is a *local* cap (this one process must stay
  cheap on its own), not a cross-process budget — there's no shared heap across profiles
  to budget against, they're separate OS processes.
- **A decision cost function**, reusing the rejected doc's §19 shape
  (`execution_frequency × estimated_compile_cost × expected_future_savings`) but with
  one change: `expected_future_savings` must be discounted to ~zero whenever the process
  is paused. There is no real future saving to a user who isn't watching — the whole
  term the naive model optimizes stops meaning anything once nobody benefits from lower
  latency.

## 4. Decision policy sketch

- **Running + hot by count:** eligible to compile. This is the one case where the
  rejected doc's original model is simply correct — reuse it unmodified.
- **Paused:** never tier up. Interpreter only, regardless of how hot a function looked
  before pausing.
- **Running → paused transition, for an already-compiled function:** evict — free the
  `CodeObject`, fall back to interpreting on the next call. This is a *policy* eviction,
  not a deopt: nothing about the function's correctness changed (no guard failed, no
  type assumption broke), so it doesn't need the rejected doc's §26 deopt machinery
  (state reconstruction mid-execution at an arbitrary pc). A lazy swap — "the next call
  to this function uses the interpreter path instead" — is enough, and much simpler.
  Whether this eviction is worth building at all, versus just letting already-compiled
  code linger until the process exits, is explicitly an open question (§6) — not decided
  here.
- **Feature flag:** the whole subsystem stays one flag away from "always interpret"
  (rejected doc §54's `atomic.js.jit=false` is the right idea, keep it). The orchestrator
  being wrong should only ever cost a missed optimization, never correctness.

## 5. Where this plugs into the existing architecture

- Background compilation still happens off the interpreter's own thread, installed
  atomically at a safepoint (rejected doc §20) — that part is orthogonal to the
  paused/running policy above and doesn't need to change.
- The orchestrator reads §2's paused/running state; it does not own or duplicate it. If
  `profile-worker` ever needs the tiering decision to react *immediately* on pause
  (rather than lazily on next call) that's a new integration point into the `PAUSE`
  handling path — not designed here, flagged in §6.

## 6. Open questions (not decided — for the scope conversation this doc is prep for)

- Is a lazy "evict on next call after pause" swap enough, or does memory actually need
  to be freed *at the moment of pausing* (i.e., `PAUSE` handling itself triggers
  eviction)? Leans toward the lazy version for a first cut — simpler, and the memory
  isn't freed instantly either way without a deliberate hook.
- Hard cap (refuse to compile past the per-process budget) vs. soft cap (evict the
  coldest already-compiled function to make room)? Leans toward hard cap — simpler,
  and this doc's whole bias is toward not building complexity until a concrete number
  proves it's needed.
- Does the existing binary paused/running signal ever need a third state (e.g. "visible
  in a non-active pane, still worth some latency investment")? No evidence for this yet
  — don't build it speculatively (§2).
- None of this is real until `ATOMIC_JS_SPIKE.md` produces a green signal. This document
  is deliberately ahead of that so the next scope conversation doesn't start from the
  rejected doc's naive model.

## 7. Explicitly not decided here

Nothing in this document authorizes building any of this. It exists so that, *if* the
spike goes green, the next scope conversation starts from an already-thought-through
design rather than from the rejected doc's V8-shaped tiering model — which, adopted
unmodified, would quietly undermine the product's own reason for existing.

---

[← back to `ATOMIC_JS_SPIKE.md`](ATOMIC_JS_SPIKE.md)
