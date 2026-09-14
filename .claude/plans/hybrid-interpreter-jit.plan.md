# Hybrid Interpreter + Simple JIT — Future Plan

Status: **speculative, not scheduled** (no P0–P5 slot yet). Written 2026-09-13 per user request to have a plan ready for later.

## Conflict with existing decision (read first)

`spec/architecture/performance.md:573-588` (§21.6) already benchmarked QuickJS-ng interpreter-only against a JIT'd swap and concluded **no case for it** — 5M+ transcendental ops/sec from the plain interpreter is way past idle-game tick needs (1-60Hz). `CLAUDE.md`'s positioning section says the defensible axis is idle memory/CPU/cold-start *per profile*, not raw JS throughput, and explicitly says not to chase beating Chromium/V8 on execution speed.

A JIT adds per-profile cost (code cache memory, larger binary, warm-up allocation) — the opposite of what "many light profiles" needs. So this plan is written as an **opt-in, feature-flagged tier**, off by default, only justified if a future real workload (not synthetic bench) shows the interpreter is actually the bottleneck. Don't build this speculatively — build it when a profiled workload demands it.

## Goal (if/when triggered)

Keep QuickJS-ng interpreter as baseline for every profile (cold code, one-shot scripts, low-frequency idle-game logic). Add a simple tiered JIT for hot numeric loops only, so the common case stays light and only genuinely hot code pays JIT cost.

```
Atomic JS
     │
  ┌──┴──┐
  │     │
Interp  Simple JIT (tier-up only)
  │       │
cold   ┌───┴────┐
code   │        │
     numeric   hot funcs
     loops     (game tick calc)
```

## Trigger condition (must be true before starting)

- A real (not synthetic) profiled workload shows JS execution — not IPC, not layout, not paint — as the dominant frame-time cost, backed by `cargo bench -p js-runtime` + flamegraph per `spec/architecture/performance.md` §22.
- The memory cost of a JIT (code cache + compiler machinery) per profile is measured and still leaves the per-profile footprint (`xtask bench-footprint`) competitive — this is the number the product's whole pitch rests on. If JIT overhead pushes per-profile RAM materially past current ~108 MiB baseline, this plan is dead regardless of speed gains.

## Design

### Tiering strategy
1. **Tier 0 — Interpreter (always)**: every function starts here. QuickJS-ng bytecode, unchanged from today.
2. **Tier 1 — Counted tier-up**: per-function call/loop-iteration counter (reuse QuickJS's existing bytecode op dispatch — add a counter increment on `OP_loop`/function entry, no new pass). Threshold crossed → queue for JIT compile on a background thread (never block the tick).
3. **Tier 2 — Simple JIT**: compile only a restricted subset — numeric loops and arithmetic-heavy functions with monomorphic types observed at tier-up time (no polymorphic inline caches, no deopt-heavy speculation like V8). If the function's types stop matching what was compiled, fall back to interpreter for that call (no re-JIT storm — cap re-compiles per function).

### Scope of what gets JIT'd (deliberately narrow)
- Pure numeric loops (`for`/`while` with number-only locals) — idle-game tick math is exactly this shape (see `hot_loop.rs` bench: accumulators + sqrt/log/%).
- No JIT for: DOM calls, string ops, object property access with megamorphic shapes, anything crossing the JS↔Rust FFI boundary (`spec/architecture/performance.md` §19 already flags that boundary as the real hotspot, not raw arithmetic — a JIT does nothing for it).

### Where it plugs into the existing crate layout
- New crate `crates/js-jit` (isolated — keeps `quickjs-sys`/`js-runtime` build simple for the common no-JIT case, and lets it be cfg'd out entirely via a Cargo feature).
- `js-runtime`'s bytecode dispatch loop gains one counter check + a call-out to `js-jit::maybe_compile(fn_id)` behind `#[cfg(feature = "jit")]` — zero-cost when the feature is off.
- Codegen target: a minimal in-process bytecode-to-native path for a fixed instruction subset (add/sub/mul/div/mod/compare/branch/call-to-known-builtin like sqrt/log) — not a general-purpose backend, not pulling in Cranelift/LLVM unless a spike proves the hand-rolled subset can't keep pace. Cranelift is the fallback if the subset approach stalls (single-dependency JIT backend, no LLVM toolchain requirement — fits the "no Node/npm, minimal toolchain" project convention).

### Safety / correctness
- Every JIT'd function must produce bit-identical results to the interpreter for the same inputs — enforce with a differential test harness: run both tiers on the same call, assert equality, on a percentage of calls in debug builds (never in release, to avoid double-execution cost).
- GC interaction: QuickJS-ng's GC assumptions (`CLAUDE.md`'s "Known gotchas" section on refcount/class-ID lifetime) must hold for JIT'd frames too — JIT'd code must go through the same `JS_FreeValue`/refcount discipline, no raw pointer arithmetic on GC'd values inside JIT'd code without holding proper refs.

## Phased rollout (only after trigger condition met)

1. **Spike**: counter instrumentation + threshold tuning, no codegen yet — measure how much time is *actually* interpreter dispatch overhead vs. real work, on the idle-game benchmark (`spec/ROADMAP.md` §21.5 shape). Kill the plan here if dispatch overhead is a small fraction of frame time.
2. **Minimal codegen**: numeric-loop-only JIT, feature-flagged, x86_64 only (match current Windows-first platform priority — see `platform-apis` being Windows-only today).
3. **Differential testing**: interpreter vs JIT equivalence harness, run in CI under the `jit` feature.
4. **Footprint re-measurement**: rerun `xtask bench-footprint` with feature on at n=1 and n=5 profiles — this is the actual go/no-go gate, not JS throughput.
5. **Opt-in default**: ship behind Settings toggle (mirrors existing GPU-adapter/FPS-cap toggles in `apps/shell`'s Settings panel) — never on by default given the memory tradeoff, until real-world data says otherwise.

## Explicit non-goals

- No general JIT competing with V8/JIT tiers (Ignition/Sparkplug/Maglev/Turbofan-equivalent) — that's a multi-year team effort, not this project's scale.
- No speculative optimization requiring heavy deopt machinery (hidden classes, polymorphic ICs) — the narrow numeric-loop subset avoids needing it.
- No JIT for the DOM/FFI boundary — that's a native-code architecture problem (§19), not solvable by JIT'ing JS.
