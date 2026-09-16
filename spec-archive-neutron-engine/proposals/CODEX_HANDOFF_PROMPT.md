# Codex handoff prompt — AtomicJS profiling state

This is a ready-to-paste prompt to hand off context to another agent (Codex) about the
`atomicjs` validation spike's current profiling/performance state. Not itself part of
the spec's decision chain — a communication artifact.

---

## Prompt

You're picking up context on **AtomicJS**, a validation-spike JS interpreter in this
repo (Atomic browser, a lightweight multi-profile browser engine). Read this in full
before doing anything — it summarizes a long profiling investigation so you don't have
to re-derive it.

### What AtomicJS is and why it exists

The product's core differentiator is running many isolated browser profiles at once
with far lower memory/CPU than Chromium-based tools. The engine uses QuickJS-ng
(`crates/js-runtime`), not V8 — deliberately, for its small/no-JIT footprint. A prior
proposal to replace QuickJS-ng with a from-scratch engine was **rejected**
(`spec/proposals/ATOMIC_JS_ARCHITECTURE.md`) as premature — but one narrow, testable
question survived: *can a minimal, purpose-built interpreter beat QuickJS-ng's
per-profile memory/cold-start footprint, without regressing execution latency too
badly?* That question is what `spec/proposals/ATOMIC_JS_SPIKE.md` scopes and what
`crates/atomicjs` implements.

**Read these four docs for full detail, in this order** (each cross-references the
others):

1. `spec/proposals/ATOMIC_JS_ARCHITECTURE.md` — the rejected full-engine proposal, kept
   as a record of why.
2. `spec/proposals/ATOMIC_JS_SPIKE.md` — the actual scoped spike: methodology,
   functional scope, step-by-step plan, **and a `## Results` section with the full
   profiling narrative and every number below**.
3. `spec/proposals/ATOMIC_JS_TIERING.md` — a contingent design (interpret-vs-compile
   orchestrator) for *if* this ever becomes a real JIT project. Not relevant unless
   you're asked about tiering specifically.
4. `spec/proposals/ATOMIC_JS_MILESTONE_2.md` — the next drafted (not yet started)
   milestone: get AtomicJS to correctly run `crates/js-runtime/benches/hot_loop.rs`'s
   real idle-tick-shaped script instead of the five toy reference programs.

### Current implementation state

- Crate: `crates/atomicjs`, a single crate, **deliberately not a member of the root
  Cargo workspace** (its own empty `[workspace]` stanza in its `Cargo.toml`) — so
  `cargo build/test --workspace` from the repo root never touches it, and it's
  removable with one `rm -rf` if the thesis fails. Build/test it from inside its own
  directory (`cd crates/atomicjs && cargo build`/`cargo test`), not with `-p atomicjs`
  from the root.
- Modules: `lexer.rs`, `ast.rs`, `parser.rs`, `bytecode.rs`, `compiler.rs`, `value.rs`,
  `vm.rs`. Public entrypoint: `atomicjs::run_source(&str) -> Result<Value, AtomicJsError>`.
- 51 tests, all passing, across `tests/{lexer,parser,compiler,vm}_test.rs`. All five
  reference programs (`sum`, `obj`, `closure`, `props`, `closures` — see
  `ATOMIC_JS_SPIKE.md` §4) produce their exact expected completion values, not just "run
  without crashing."
- Comparison binaries for benchmarking: `crates/atomicjs/examples/atomicjs-run.rs` and
  `crates/js-runtime/examples/quickjs-run.rs` — both take one CLI arg naming a reference
  program, print `elapsed_us=... maxrss_bytes=... result=...` in an identical shape.

### The profiling investigation (what actually happened, in order)

1. **First real cross-engine numbers** (release builds, `hyperfine --warmup 5 --runs
   50`): `sum` 1.22× slower than QuickJS-ng, memory ~6.5× smaller. `props` **5.08×**
   slower (red flag). `closures` **2.87×** slower (also over the 2× guardrail this spike
   uses).
2. **Profiled `props` with `samply`** (had to rebuild with
   `--config profile.release.debug=true --config profile.release.strip=false` — the
   plain release build had no debug info to symbolicate against — and
   `--unstable-presymbolicate`, since `samply record --save-only` doesn't bake symbol
   names into the saved profile by default). Found a real bug, not just "HashMap is
   inherently slower": `Instr::GetProp` cloned the property-name `String` out of the
   constant pool on *every single access*, even though it's a compile-time constant
   already borrowable for the whole call's lifetime.
3. **Fixed it** (borrow `&str` instead of cloning). Re-measured: `props` dropped from
   5.08× to **1.89×** — inside the 2× guardrail.
4. **Profiled `closures`** (doesn't use `GetProp` at all — different bottleneck):
   `execute_function` allocated a fresh `Vec<LocalSlot>` (locals) and `Vec<Value>`
   (operand stack) on *every single function call*, and `closures`' loop calls the same
   closure 1,000,000 times — 2,000,000+ heap allocations attributable purely to the
   calling convention.
5. **Fixed it**: added `VmPools`, a free-list of reusable buffers shared across the
   recursive call tree via `RefCell`, checked out at call entry and returned (cleared,
   not deallocated) on the `Instr::Return` success path. Re-measured: `closures` dropped
   from 2.87× to **~1.88×**.
6. **Re-profiled again**, found a smaller remaining allocation: `Instr::Call` still
   collected popped arguments into a fresh `Vec<Value>` before handing them to the
   callee. Pooled that too (`VmPools` gained an `args` free-list, `execute_function`
   now takes `args: &[Value]` instead of an owned `Vec<Value>`). **Re-measured: no
   clearly attributable improvement** — this allocation is much smaller (0-1 elements
   typically) than the locals/stack one, and its effect (if any) is smaller than the
   ~20-50% run-to-run variance this sandbox already shows. Applied anyway (correct,
   free, one fewer real allocation) but not claimed as a fix for anything.

### Current numbers (final consolidated round, all three back-to-back, same session)

| Program | Slowdown vs. QuickJS-ng | Memory ratio |
| --- | --- | --- |
| `sum` | ~1.0-1.3× | ~6.5× smaller |
| `props` | ~1.9-2.15× (oscillates across sessions) | ~6.4× smaller |
| `closures` | ~1.7-2.1× (oscillates across sessions) | ~6.2× smaller |

**Important caveat, stated explicitly in `ATOMIC_JS_SPIKE.md`'s Results section**: this
sandbox is not a dedicated benchmarking machine. `hyperfine` flagged statistical outliers
repeatedly. `props` and `closures` sit right at the 2× guardrail and have measured on
both sides of it across repeated sessions — this is reported as a genuinely open,
noise-bound question, not resolved in either direction. A clean re-measurement on a
quiet, dedicated machine is what would actually settle it.

### Current verdict

**Green, conditional** (decided 2026-09-14, at the user's explicit request to call it
rather than defer further) — memory holds decisively and consistently; execution time
is comfortably fine on one of three programs and genuinely borderline (not a clear
failure like the original 5.08×/2.87× numbers) on the other two. Reasoning recorded in
full in `ATOMIC_JS_SPIKE.md`'s final verdict section, including why the borderline
result is a weaker signal than it would be for a real (much lighter) idle-game workload
— `spec/architecture/performance.md` §21.6 already showed QuickJS-ng has large
throughput headroom (5M+ ops/sec) against actual 1-60Hz tick needs, and these reference
programs are deliberately heavier synthetic loops (1,000,000 iterations each) than any
real target content would be.

Green here does **not** mean authorization to wire AtomicJS into production
(`crates/js-runtime`/`profile-worker`) — that's an explicitly separate, larger decision
not made yet.

### What's being asked of you

Get full context from this and the four docs above before doing or suggesting anything.
If you're being asked to continue this work, `ATOMIC_JS_MILESTONE_2.md` is the drafted
next step (no deadline, scope-bounded instead) — read it before proposing what to build
next.
