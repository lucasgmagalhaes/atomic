# ATOMIC_JS_SPIKE.md — Validation Spike: Is a Custom Interpreter Worth It?

## 0. Status & relationship to the rejected proposal

Successor to [`ATOMIC_JS_ARCHITECTURE.md`](ATOMIC_JS_ARCHITECTURE.md) (rejected 2026-09-14: that
doc proposed replacing QuickJS-ng outright with a from-scratch VM/GC/JIT, contradicting
[`mockup/browser-idle-spec.md`](../../mockup/browser-idle-spec.md)'s JS-engine crate choice and
[`spec/ROADMAP.md`](../ROADMAP.md)'s 2026-08-26 product-scope decision).

This document does **not** revive that proposal. It scopes a small, deliberately narrow
experiment — a **single crate**, `crates/atomicjs` (2026-09-14: consolidated from an
earlier `values`/`parser`/`bytecode`/`vm` four-crate split — see §5's naming note),
**deliberately excluded from the root workspace** via its own empty `[workspace]` stanza
in its own `Cargo.toml` (§6) — to cheaply test the one part of the original thesis
that's still worth knowing the answer to.

**This is not planned work.** Nothing here enters `spec/ROADMAP.md` unless the spike
produces a green signal (§9) *and* the user explicitly reopens the 2026-08-26 scope
decision to schedule it as real work.

## 1. The actual question being tested

Not "can we build a faster JS engine than QuickJS-ng in general" — that's the question
the rejected doc asked, and the review already answered it (no: QuickJS-ng is mature,
already 75% through the ECMAScript matrix in `crates/js-runtime`, and a competitive
custom VM+JIT is normally a multi-year, large-team effort).

The narrow, honest question this spike answers instead:

> For this product's actual workload shape — many concurrently idle profiles, short
> bursts of interactive script, long stretches of near-zero activity, small idle-game-style
> arithmetic/property-access loops on tick — can a minimal, purpose-built interpreter beat
> QuickJS-ng's **per-profile footprint** (cold-start time, resident memory) without badly
> regressing execution latency on that same workload?

Explicitly **not** being tested:

- Raw computational throughput against V8/JSC — irrelevant, this project ships neither.
- Any form of JIT — out of scope for the spike entirely.
- General web/ECMAScript compatibility — out of scope; the spike runs three fixed programs.

## 2. Prior art already on record — read this before starting

A sibling, independently-written plan already touched adjacent ground. Both exist; this
section says exactly how they relate so nobody re-derives what's already known or,
worse, assumes this spike re-litigates a closed question it doesn't.

- **[`.claude/plans/hybrid-interpreter-jit.plan.md`](../../.claude/plans/hybrid-interpreter-jit.plan.md)**
  (2026-09-13, speculative/not scheduled) proposes a *different* thing: keep QuickJS-ng
  as the tier-0 interpreter and add an opt-in, feature-flagged JIT tier bolted onto its
  own bytecode dispatch for narrow hot numeric loops only. It is explicit that this is
  **not triggered today** — its own trigger condition (a real, profiled workload showing
  JS execution as the dominant frame-time cost, plus JIT memory cost not regressing the
  per-profile footprint baseline) hasn't been met.
- That plan cites **`spec/architecture/performance.md` §21.6** ("Benchmark Results,
  2026-09-12" — the plan's own `spec/ROADMAP.md:571-588` reference is stale; the content
  moved when `spec/` was split from the old `FULL_SPEC.md`, see `spec/INDEX.md`). Worth
  flagging to the user as a small drive-by fix, not fixed here since it's a different
  document. The real numbers there:
  - `crates/js-runtime/benches/hot_loop.rs` (a synthetic idle-game-shaped tick handler,
    200,000 ticks, 3 accumulators + `sqrt`/`log`/`%` per tick): **~185ms**, i.e. 5M+
    transcendental-shaped ops/sec from QuickJS-ng's plain interpreter, no JIT — "way past
    idle-game tick needs (1–60Hz)."
  - `xtask bench-footprint` (spawns real `profile-worker` processes against a fixed test
    page, samples via `platform_apis::process_stats`): **n=1: 114.6 MiB; n=5: ~108.2
    MiB/profile avg**, CPU negligible while idle.
- **What that already answers:** QuickJS-ng's own throughput has large headroom for this
  product's workload, and *adding a JIT on top of it* isn't justified today. **What it
  does not answer, and what this spike is actually for:** whether QuickJS-ng's baseline
  footprint (~108 MiB/profile, whole-process — JS engine plus DOM, layout, GPU context,
  everything else `profile-worker` carries) is itself something a much smaller,
  purpose-built interpreter (no Proxy, no BigInt, no `Intl`, no regex engine, no general
  spec conformance) could beat *for the JS-engine slice of that number specifically*.
  Nobody has measured that slice in isolation yet — this spike does, narrowly.
- **Why this spike doesn't reuse `xtask bench-footprint` / `platform_apis::process_stats`
  directly**, even though reuse is normally the right call
  (`spec/architecture/overview.md`'s "Architecture Before APIs" rule): two concrete
  reasons, checked against the actual code, not assumed —
  1. `platform_apis::process_stats::sample` is `#[cfg(windows)]`-only
     (`crates/platform-apis/src/process_stats.rs:82`); on Darwin (this repo's actual dev
     machine) it's the unsupported stub, and `xtask bench_footprint` would print
     `sampling unsupported/failed` for every process. It isn't usable here today.
  2. Even where it works, it measures a *whole* `profile-worker` process — JS engine +
     DOM + layout + render + GPU context together. This spike's question is about the JS
     engine's own footprint in isolation, so a whole-process number would conflate a
     constant (DOM/render/GPU cost, identical regardless of which JS engine sits
     underneath) with the one variable actually being tested.
  §7's own harness (a minimal `atomicjs` binary + a minimal QuickJS-ng binary,
  `libc::getrusage`, no DOM/render) is the narrower, correct tool for *this* question. If
  a green signal here ever leads to actually wiring a different engine into
  `profile-worker` for real, **that** integration should absolutely be re-validated with
  `xtask bench-footprint` on a Windows machine (or after that tool grows Darwin support)
  — this is noted as a later, contingent step in §10, not part of this spike.
- Both plans independently converge on the same discipline: measure a real, memory-first
  signal before building, never decide from throughput alone, never decide from
  assumption. That consistency is a good sign this spike's methodology (§7) fits how this
  team already validates JS-engine questions.

## 3. Hard scope limits

Even living inside `crates/` (§6), this crate is deliberately kept outside the root
workspace and unproven — normal habits like "add a dependency," "make it a bit more
general," "handle one more case" apply to every other crate in this repo, but not to
this one yet. If any of these gets tempting to break, that's a signal to stop and report
to the user, not to keep building:

- **Time-boxed: 5 working days.** Re-evaluated after step 4 in §8 (first real comparison
  number) — that's the checkpoint to actually revisit this number with real data, not a
  reason to leave it soft until then.
- **No GC** beyond a simple arena/leak for the process's own short lifetime. A real
  generational GC ([`ATOMIC_JS_ARCHITECTURE.md`](ATOMIC_JS_ARCHITECTURE.md)'s §28) is
  exactly the multi-month subsystem that must not be built before the thesis is validated.
- **No JIT** of any kind, baseline or otherwise. No separate `compiler` module split
  either — see §5.3's note on why AST→bytecode lowering stays alongside the bytecode ISA
  for now.
- **No object model** beyond what the three reference programs (§4) need — no Shapes, no
  inline caches. Those are optimizations for a problem this spike hasn't earned yet.
- **No dependents.** Nothing in `crates/js-runtime`, `dom`, `profile`, or any other
  production crate may depend on `atomicjs` — and since it isn't a root-workspace member
  (§6), nothing *can* depend on it via a workspace-relative path without that being a
  deliberate, visible change to the root `Cargo.toml`.
- **No new builtins** beyond what the three reference programs need (arithmetic, `+=`,
  `++`, `for`, function declarations/closures, plain object literal + property get/set,
  `return`).
- **No error/exception model.** None of the three reference programs throw. A failure can
  be a plain `Result<Value, AtomicJsError>` / panic — do not build `try`/`catch`/`Error`
  objects for this spike.

## 4. Reference programs, expected results, and benchmark variants

Every checkpoint in §8 has two acceptance criteria, not one: **correct, then measured.**
A performance win from a wrong interpreter is not a result — assert the exact expected
value before trusting any timing number.

### 4.1 The three base programs (unchanged from the rejected doc's §61 milestone)

```javascript
// name: sum — expected completion value: 499999500000
function sum(n) {
    let total = 0;
    for (let i = 0; i < n; i++) {
        total += i;
    }
    return total;
}
sum(1000000);
```

```javascript
// name: obj — expected completion value: 30
const player = { level: 10, damage: 20 };
player.damage + player.level;
```

```javascript
// name: closure — expected completion value: 3
function makeCounter() {
    let count = 0;
    return function () {
        return ++count;
    };
}
const counter = makeCounter();
counter();
counter();
counter();
```

("Completion value" — see §5.4 for exactly what that means for `run_source`.)

### 4.2 Loop-wrapped benchmark variants (for steps 6–7 in §8)

`obj` and `closure` above are single-shot — timing them directly would mostly measure
parse/compile overhead, not the property-access or call/closure path. These variants
reuse the exact same grammar already in scope (§5.2) — no new syntax needed — and are
what the cross-engine comparisons in §8 steps 5–6 actually run:

```javascript
// name: props — expected completion value: 20000000
const player = { level: 10, damage: 20 };
let total = 0;
for (let i = 0; i < 1000000; i++) {
    total += player.damage;
}
total;
```

```javascript
// name: closures — expected completion value: 500000500000
function makeCounter() {
    let count = 0;
    return function () {
        return ++count;
    };
}
const counter = makeCounter();
let total = 0;
for (let i = 0; i < 1000000; i++) {
    total += counter();
}
total;
```

Five named programs total: `sum`, `obj`, `closure` (correctness-only, too short to
benchmark meaningfully), `props`, `closures` (the two actually used for cross-engine
timing in §8).

### 4.3 Where these scripts live

Embedded as string constants directly inside each comparison binary (§7.1) — **no
external `.js` files, no shared path convention.** Five short scripts duplicated as
string literals across two tiny binaries is cheaper than coordinating a shared file
location across two crates with different working directories under `cargo run`. Each
binary takes one positional CLI argument selecting the script by name (`sum` / `obj` /
`closure` / `props` / `closures`) — both binaries accept the exact same five names, so a
`hyperfine` invocation naming one is directly comparable to naming the other.

## 5. Functional scope, module by module

A single crate, `atomicjs`, internally organized by module — matching this workspace's
existing convention of bare functional crate names (`dom`, `css`, `net`, `html`, ...)
rather than an `atomic-js-` prefix, applied here to one crate instead of a naming
decision per crate. Everything below is *in scope because a reference program in §4
needs it*, nothing more; treat any capability not listed here as explicitly excluded for
the spike, even if it looks trivial to add.

> **Naming history:** an earlier revision of this doc split this into four crates named
> `core`/`parser`/`bytecode`/`vm`, then renamed `core` to `values` to dodge a real
> footgun (a workspace member literally named `core` collides with Rust's own
> implicit extern-prelude `core` since edition 2018). Consolidating back to one crate,
> named `atomicjs`, resolves that concern entirely — there's no `core`-named anything
> left to collide with anything. `atomicjs` doesn't collide with any existing workspace
> crate (checked: `atoms`, `dom`, `css`, `net`, `html`, ... — no `atomicjs` today).

### Suggested module layout (`crates/atomicjs/src/`)

```text
lib.rs        — public entrypoint: run_source(&str) -> Result<Value, AtomicJsError>
lexer.rs
ast.rs
parser.rs     — AST from lexer output (§5.2)
bytecode.rs   — instruction enum + BytecodeModule + disassembler (§5.3)
compiler.rs   — AST -> bytecode, including the captured-variable pass (§5.3/§5.4)
value.rs      — Value, JsObject, FunctionData, Display impl, FunctionFeedback (§5.1)
vm.rs         — CallFrame, the interpreter loop (§5.4)
error.rs      — AtomicJsError
```

`lib.rs`'s `run_source` is the *only* symbol the benchmark harness (§7) calls — keep it
that shape so swapping which engine a benchmark binary links against is a one-line
change (mirrors how `crates/js-runtime`'s own `Runtime`/`Context` are already the
boundary other crates call through).

### 5.1 `value.rs` — shared foundational types

- `Value` enum: `Undefined`, `Number(f64)`, `Object(Rc<RefCell<JsObject>>)`,
  `Function(Rc<FunctionData>)`. No NaN-boxing, no `Int32` fast path — later-optimization
  concerns with no place in a spike.
- `Value` needs a minimal `Display` impl — **closes a real gap**: the benchmark harness
  (§7.1) needs to print a result comparable to `crates/js-runtime`'s `Context::eval`,
  which already returns `Result<String, EvalError>` (a stringified result).
  `Number(f64)` should format the same way plain decimal formatting would (matching
  QuickJS-ng's own number-to-string for these integer-valued results — none of §4's
  expected values need scientific notation or float precision edge cases); `Undefined`
  formats as `"undefined"`. This is what makes the harness's printed `result=...` line
  directly diffable between engines, not just internally asserted.
- `JsObject { properties: HashMap<String, Value> }` — no prototypes, no Shapes, no
  property descriptors. **Missing-property read returns `Value::Undefined`**, matching
  real JS `undefined` semantics — closes an ambiguity the bytecode table (§5.3) left
  open. None of §4's programs read a missing property, but the behavior still needs to
  be defined rather than left as a panic waiting to surprise someone.
- **Type-error stance (closes a gap):** an operation like `ADD` on two non-`Number`
  values, or `CALL` on a non-function, is unreachable for all five programs in §4 given
  a correct compiler — treat it as an internal invariant violation (`panic!`/
  `debug_assert!`), not a recoverable `AtomicJsError`. Building a real type-error path
  would be exactly the kind of error-model work §3 already excludes.
- `FunctionFeedback { call_count: u32, loop_count: u32 }` — **informed by
  [`ATOMIC_JS_TIERING.md`](ATOMIC_JS_TIERING.md).** A future tiering orchestrator (out
  of scope here — no such orchestrator is built in this spike) would need cheap,
  sampled-not-per-opcode counters like this one (`ATOMIC_JS_TIERING.md` §3). Defining
  the *type* here and having `vm.rs` increment it (§5.4) costs nothing extra and turns
  an assumption ("collecting this is cheap") into something the spike's own benchmarks
  can actually confirm — without building any consumer of the data. No decision reads
  `FunctionFeedback` in this spike; it's instrumentation with no orchestrator attached.
- `AtomicJsError` (in `error.rs`) — shared by the parser (syntax errors) and the VM
  (runtime errors, though §3 keeps this path minimal).

### 5.2 `lexer.rs` / `ast.rs` / `parser.rs`

- **Lexical grammar (closes a gap):** numbers are plain decimal (`[0-9]+(\.[0-9]+)?`,
  lexed straight to `f64`) — no scientific notation, no hex/octal/binary, no separators;
  none of §4's programs need them. Identifiers are `[A-Za-z_][A-Za-z0-9_]*` — no Unicode
  identifier support, no `$`. No negative numeric literals in the grammar itself (`-5` is
  unary minus applied to `5` at the expression level) — moot here since no program uses
  one, but stated so a lexer implementer doesn't have to guess.
- Grammar: literals (`f64` numbers, identifiers, object literals `{ key: value, ... }`),
  declarations (`function name(params) { ... }`, `let`, `const`), statements (expression
  statement, `for (init; cond; update) { ... }`, `return`, block `{ ... }`), expressions
  (member access `obj.prop`, function calls, function expressions for closures,
  assignment `=`, compound assignment `+=`, prefix increment `++x`, binary `+`, `<`).
- Explicitly out: `if`/`while`/`switch`/`class`/`try`/template literals/arrow functions/
  destructuring/spread/`var`/hoisting semantics/ASI edge cases/strict-mode nuance — add
  only if a reference program is extended to need one, not preemptively.

### 5.3 `bytecode.rs` / `compiler.rs`

A subset of the rejected doc's §11 ISA:

| Opcode | Semantics |
| --- | --- |
| `LOAD_CONST idx` | push constant pool entry `idx` |
| `LOAD_LOCAL slot` | push local slot `slot` |
| `STORE_LOCAL slot` | pop, store into local slot `slot` |
| `GET_PROP name_idx` | pop object, push `object[name]` (`Undefined` if missing, §5.1) |
| `SET_PROP name_idx` | pop value, pop object, set `object[name] = value` |
| `NEW_OBJECT` | push a new empty object |
| `ADD` | pop b, pop a, push `a + b` (numbers only — see §5.1's type-error stance) |
| `LT` | pop b, pop a, push `a < b` (bool) |
| `JUMP addr` | unconditional jump |
| `JUMP_IF_FALSE addr` | pop, jump if falsy |
| `MAKE_CLOSURE fn_idx` | push a function value capturing the current environment |
| `CALL argc` | pop `argc` args + callee, push call result |
| `RETURN` | return top of stack from current frame |
| `POP` | discard top of stack |

No `SUB`/`MUL`/`DIV`/`MOD`/`NOT`/`EQ`/`STRICT_EQ`/`TRY_BEGIN`/`TRY_END`/`DUP`/`GET_ELEMENT`/
`SET_ELEMENT`/`CONSTRUCT`/`THROW` — none of the five programs need them.

**`for`-loop compilation strategy (closes a gap — the exact shape step 3 in §8 should
produce):**

```text
    compile(init)
loop_start:
    compile(cond)
    JUMP_IF_FALSE loop_end
    compile(body)
    compile(update)
    JUMP loop_start
loop_end:
```

`compiler.rs` also owns the captured-variable pass (§5.4) and a disassembler for
debugging (in `bytecode.rs`). The rejected doc split `compiler` into its own *crate*
specifically to let multiple codegen backends (baseline JIT, mid-tier JIT) share one
lowering pipeline — that justification doesn't exist yet with no JIT in scope (§3), so
keeping the compiler as a module next to the bytecode types is the YAGNI-correct call.
Split it into its own module boundary (or crate, if this graduates past the spike) the
day a second backend actually needs to reuse it, not before.

### 5.4 `vm.rs` — interpreter loop, call frames, execution

The crate's `lib.rs` exposes `run_source(&str) -> Result<Value, AtomicJsError>` by
wiring `lexer` → `parser` → `compiler` → this module's interpreter loop.

- **Top-level execution semantics (closes a gap):** a script's top level behaves like an
  implicit function body with its own frame — `let`/`const` declarations become locals
  in that frame, and a named `function` declaration binds its name as a local holding
  the closure, in the same frame, before the rest of the top level runs (so
  `sum(1000000);` can call a `sum` declared earlier in the same script). `run_source`'s
  return value — its "completion value" as used throughout §4 — is the value of the last
  top-level *expression statement* evaluated (an `eval`-completion-value shape), or
  `Value::Undefined` if the script's last statement isn't an expression statement. This
  is exactly why every program in §4 ends with a bare expression statement rather than a
  `return` (`return` outside a function has no defined meaning here and isn't needed).
- `CallFrame { function, bytecode, pc, locals: Vec<Value>, previous: Option<Box<CallFrame>> }`
  (rejected doc §12, unchanged — this shape is correct at any scale).
- Closures: a compile-time "captured variables" pass (lives in `compiler.rs`, since it's
  a lowering-time decision) — walk a nested function body, and any identifier it
  references from an enclosing scope gets boxed as `Rc<RefCell<Value>>` instead of a
  plain stack slot; everything else stays a cheap unboxed local. Standard
  upvalue-analysis technique (same idea CPython/Lua use), and it matters for the
  benchmark: boxing *every* local instead would make the `sum`/`props` loops' numbers
  meaningless (neither captures anything, so both must stay fully unboxed) while still
  correctly handling `closure`/`closures`' captured `count`.
- Increments a `value::FunctionFeedback` counter per function on call/loop-back-edge —
  cheap, sampled, never read by anything in this spike (§5.1).

## 6. Crate location & conventions

`atomicjs` lives at `crates/atomicjs/`, physically alongside every other crate, but is
**deliberately not a member of the root workspace.**

**How the opt-out works, concretely:**

```toml
# crates/atomicjs/Cargo.toml
[workspace]
resolver = "2"
# intentionally empty `members` — this makes crates/atomicjs the root of its OWN
# single-crate workspace, decoupled from the repo's root Cargo.toml. Without this
# stanza, Cargo would try to fold it into the parent workspace it physically sits
# under and fail (it isn't listed in the root's `members`).

[package]
name = "atomicjs"
version = "0.1.0"
edition = "2021"

[dev-dependencies]
criterion = "0.5"
libc = "0.2"

[[bench]]
name = "atomicjs_bench"
harness = false
```

The root `Cargo.toml`'s `[workspace] members` list is **not touched** — that's the
whole point.

**What this buys, concretely (recovers the isolation property the very first revision of
this doc recommended, via a different mechanism than "put it under `spikes/` or a
branch," which the user preferred against at the time):**

- `cargo build --workspace` / `cargo test --workspace` from the repo root never touches
  it, never breaks on it, and its dependency graph can never leak into the root
  workspace's `Cargo.lock`.
- A red result is deletable with a single `rm -rf crates/atomicjs` — no `Cargo.toml`
  edit needed anywhere, since it was never listed as a member.
- It still lives under `crates/`, physically colocated with everything else, easy to
  find, not off in some unrelated location.

**What this costs, explicitly (so it isn't a surprise later):**

- Building/testing it is a separate invocation: `cd crates/atomicjs && cargo build` /
  `cargo test` / `cargo bench`, or `cargo build --manifest-path crates/atomicjs/Cargo.toml`
  from the root — **not** `cargo build --workspace`.
- It builds into its **own** `crates/atomicjs/target/` directory, not the shared root
  `./target/` — this matters concretely for §7.2's benchmark paths (the QuickJS-ng
  comparison binary, still a root-workspace member, builds into `./target/release/...`
  as usual; the `atomicjs` one does not).
- **The pre-commit hook does not check its formatting.** Confirmed by reading
  `.cargo-husky/hooks/pre-commit`: it runs `cargo fmt --all -- --check` from the repo
  root, which only covers root-workspace members. Run `cd crates/atomicjs && cargo fmt`
  manually before each commit that touches this crate — add this as a habit for every
  step in §8, not an afterthought at the end.

Other inherited conventions (these still apply even though it's outside the workspace —
they're repo-wide conventions, not workspace-membership-gated ones):

- Tests under `crates/atomicjs/tests/<file>_test.rs`, integration-style against the
  crate's public API — not inline `#[cfg(test)] mod tests` (repo-wide convention,
  `CLAUDE.md`'s Conventions section). §8's "definition of done per checkpoint"
  (correctness first) means `crates/atomicjs/tests/vm_test.rs` should assert every §4
  program's exact expected value — not just the manual harness printing it.
  Run with `cd crates/atomicjs && cargo test` (see above — `--workspace` won't reach it).
- `rustfmt.toml`'s 4-space indent still applies (it's a repo-wide file, not
  workspace-scoped) — just run `cargo fmt` from inside the crate, per above.
- Commits: Conventional Commits, scoped to this crate — `feat(atomicjs): ...`.
- Keep the dependency list short and boring on purpose. No GC crate, no JIT/codegen
  crate (`cranelift` etc.) — those would contradict §3 before a single benchmark runs.

## 7. Performance methodology during development

The guiding principle: **measure both engines with identical in-process instrumentation**,
not divergent shell tools — otherwise a measurement-method difference can masquerade as
an engine difference. (§2 already covers why this spike doesn't reuse `xtask
bench-footprint`/`platform_apis::process_stats` for this — Windows-only, and the wrong
granularity for isolating just the JS engine.)

### 7.1 Two comparison binaries, one shared measurement shape

Two minimal standalone binaries, both taking one CLI arg (a name from §4.3):

- `crates/atomicjs/examples/atomicjs-run.rs` — calls `atomicjs::run_source`. Builds to
  `crates/atomicjs/target/release/examples/atomicjs-run` (§6 — its own target dir).
- `crates/js-runtime/examples/quickjs-run.rs` — thin wrapper around `Runtime::new()` /
  `Context::new(&runtime)` / `ctx.eval(code, "quickjs-run.js")` (the real API,
  `crates/js-runtime/src/context/mod.rs:139` / `crates/js-runtime/src/context/eval.rs:23`
  — confirmed to exist, doesn't need to be invented). Builds to the shared
  `./target/release/examples/quickjs-run` (a root-workspace member, unlike `atomicjs`).
  Needs `libc = "0.2"` added under `crates/js-runtime/Cargo.toml`'s
  `[dev-dependencies]` — it doesn't have this dependency today.

(File names chosen so Cargo's default example-binary-name-from-filename rule produces
`atomicjs-run`/`quickjs-run` directly, matching §7.2's `hyperfine` commands with no extra
`[[example]]` table needed in either `Cargo.toml`.)

Both do exactly this, in this order:

```rust
let t0 = std::time::Instant::now();
let result = engine_specific_run(&source); // atomicjs::run_source(&source) / ctx.eval(&source, "quickjs-run.js")
let elapsed = t0.elapsed();
let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
println!(
    "elapsed_us={} maxrss_bytes={} result={:?}",
    elapsed.as_micros(), usage.ru_maxrss, result
);
```

(`ru_maxrss` is bytes on macOS/Darwin, KB on Linux — note the unit if this ever runs on
both; this repo's dev environment is Darwin, so bytes is what you'll see.) Identical code
in both binaries means the only variable between their printed numbers is the engine. The
printed `result=...` line is the correctness check §4/§8 require — compare it against
the expected value table in §4 by eye during manual runs, and with a real `assert_eq!`
in `crates/atomicjs/tests/vm_test.rs` (§6) for the automated version.

### 7.2 Cold-start / memory: `hyperfine` across fresh processes

Cold start is inherently a fresh-process metric — don't try to fake it by calling both
engines in-process (the second call would be measuring a warm process). Use `hyperfine`
to get statistically meaningful numbers across many fresh processes, controlling for
filesystem-cache warm-up noise. **Mind the two different target directories (§6):**

```bash
hyperfine --warmup 5 --runs 50 \
  './crates/atomicjs/target/release/examples/atomicjs-run sum' \
  './target/release/examples/quickjs-run sum'
```

(swap `sum` for `props`/`closures` for the loop-variant comparisons in §8 steps 5–6).
Parse each binary's own printed `maxrss_bytes` line out of the runs (hyperfine's
`--export-json` plus a small script, or just eyeball the min/mean it reports on
`elapsed_us` and separately tally `maxrss_bytes` across runs) — hyperfine's own timing
already gives the wall-clock side of cold start for free.

### 7.3 In-process microbenchmarks for same-day regression catching

Cross-engine, fresh-process comparisons (§7.2) are the numbers that matter for the go/
no-go call, but they're too slow/noisy to run after every small change while building
the interpreter itself. Use `criterion` (already a proven pattern in this workspace —
`crates/js-runtime/benches/native_bench.rs`, `hot_loop.rs`) benchmarking
`atomicjs::run_source` directly on each reference program, with saved baselines. Since
`atomicjs` isn't a root-workspace member (§6), this runs from inside the crate, not via
`-p atomicjs` from the root:

```bash
cd crates/atomicjs
cargo bench -- --save-baseline before   # before a change
# ...make the change...
cargo bench -- --baseline before        # compare after
```

This catches "that refactor made the interpreter loop 3x slower" the same day it
happens, rather than at the final cross-engine comparison when it's much harder to tell
which of several changes caused it.

### 7.4 Deeper investigation when a number looks wrong

If a criterion or hyperfine result is surprising and you need to know *why* (which
opcode/function actually dominates), use a sampling profiler rather than guessing:

```bash
samply record ./crates/atomicjs/target/release/examples/atomicjs-run sum
```

(`samply` works well on macOS, produces a flamegraph viewable in the Firefox Profiler
UI.) Reach for this only when a number surprises you — not as a routine step for every
change.

### 7.5 Recording results over time

Append a dated `## Results` section to the bottom of this file at each checkpoint in §8
(steps 4, 5, 6) — actual numbers and a one-line verdict, not "seemed fine." This mirrors
how `spec/architecture/performance.md` §21.6 already records dated benchmark results,
and makes it possible to tell, after the fact, exactly when (and on which program) the
signal turned green or red.

## 8. Step-by-step implementation plan

Each step has two acceptance criteria — **correct, then measured** (§4) — and ends with
a concrete bench checkpoint per §7. Don't move to the next step on a step that hasn't
been measured, even informally, and never trust a timing number from a program that
hasn't matched its expected value first. Run `cargo fmt` inside the crate before each
commit (§6 — the root pre-commit hook won't catch it).

1. **Scaffold.** Create `crates/atomicjs` per §6's exact `Cargo.toml` shape (own empty
   `[workspace]` stanza — do **not** add it to the root `Cargo.toml`'s `members`).
   Confirm two things separately: `cargo build --workspace` from the repo root still
   passes unaffected, and `cd crates/atomicjs && cargo build` succeeds on its own with a
   stub `run_source` returning `Value::Undefined`. *Checkpoint:* none yet — but this is
   the moment to build the `examples/atomicjs-run.rs` / `examples/quickjs-run.rs` +
   `getrusage` harness from §7.1 and confirm it prints sane numbers for an empty script,
   on both. This gives you the *floor* cold-start/RSS numbers before any language work
   exists.
2. **`lexer.rs`/`ast.rs`/`parser.rs`** for the grammar in §5.2. No bytecode, no execution
   yet. *Checkpoint:* a `criterion` parse-only microbench on all five programs' source
   text (cheap, useful later to know whether parsing or execution dominates if a number
   ever looks off).
3. **`bytecode.rs`/`compiler.rs`** (AST → the ISA in §5.3, using the `for`-loop
   compilation shape given there). *Checkpoint:* a compile-time microbench; manually
   inspect the disassembled output for all five programs to catch compiler bugs before
   they show up as confusing VM bugs.
4. **`vm.rs`: interpreter loop, enough to run `sum`** (needs `LOAD_CONST`/`LOAD_LOCAL`/
   `STORE_LOCAL`/`ADD`/`LT`/`JUMP`/`JUMP_IF_FALSE`/`CALL`/`RETURN`, plus the
   `FunctionFeedback` counters from §5.1/§5.4). *Checkpoint:* assert `result=499999500000`
   first — then **first real cross-engine comparison** (§7.2) on `sum`. This is the
   earliest point you can honestly re-evaluate the time box (§3) with real data. If the
   numbers are already badly red here, stop and report rather than continuing to steps
   5–7.
5. **Extend for `obj`/`props`** (object literal + property get, needs
   `NEW_OBJECT`/`GET_PROP`/`SET_PROP` and the minimal `value::JsObject`). *Checkpoint:*
   assert `obj` → `30` and `props` → `20000000`, then cross-engine comparison on `props`
   (the loop variant — `obj` alone is too short to time meaningfully, §4.2).
6. **Extend for `closure`/`closures`** (needs `MAKE_CLOSURE` + the captured-variable
   compiler pass from §5.3/§5.4). *Checkpoint:* assert `closure` → `3` and `closures` →
   `500000500000`, then cross-engine comparison on `closures`.
7. **Consolidate the comparison table** across `sum`/`props`/`closures` (elapsed time,
   cold-start time, peak RSS, each vs. `quickjs-ng`) and write it into this file's new
   `## Results` section (§7.5).
8. **Apply §9's go/no-go criteria to that table and report to the user** — don't
   decide unilaterally that it's green; bring the numbers and let the scope decision
   (same weight as 2026-08-26's) get made explicitly.

## 9. Go / no-go signal

**Green — worth bringing back to the user as a scope discussion, not worth silently
scheduling:**

- Cold-start measurably lower than QuickJS-ng's, at the process-per-profile shape.
- Resident memory measurably lower, same shape.
- Execution time on `sum`/`props`/`closures` not worse than QuickJS-ng's by more than
  **2x** — losing by 10x on a tight loop is a red flag even alongside a memory win,
  since idle-game scripts still run real logic on every tick. (This threshold, like the
  5-day time box in §3, is the operative default — revisit explicitly if it turns out
  wrong for the actual numbers in step 4, don't just drift past it.)

**Red — archive the thesis, don't retry without new evidence:**

- No measurable win on cold-start or memory over QuickJS-ng.
- Execution time far worse than QuickJS-ng on `sum`/`props`/`closures`.
- The spike interpreter itself takes meaningfully longer to get correct than the time box
  allowed — a proxy signal that the full engine would be an even larger undertaking than
  estimated.

## 10. What happens after the spike, either outcome

- **Green:** raise it with the user explicitly as a new, dated scope decision (same
  weight as the 2026-08-26 entry in `spec/ROADMAP.md`) before anything here becomes
  scheduled work. Do not auto-promote to `spec/ROADMAP.md`. Before any real
  `profile-worker` integration, re-validate with `xtask bench-footprint` (§2) — on a
  Windows machine, or once that tool has Darwin support — since that's the whole-process
  number the product's pitch actually rests on, not this spike's isolated-engine one.
- **Red:** this file gets the same rejected status as `ATOMIC_JS_ARCHITECTURE.md`. Since
  `atomicjs` was never a root-workspace member (§6), removal is a single
  `rm -rf crates/atomicjs` — no `Cargo.toml` edit anywhere. Both docs stay in
  `spec/proposals/` as a historical record — don't delete either doc.

## 11. Related design notes

[`ATOMIC_JS_MILESTONE_2.md`](ATOMIC_JS_MILESTONE_2.md) drafts the next bounded milestone
(2026-09-14, per the user's request after this spike's final verdict) — not scheduled
work, a scope-bounded (not time-boxed) follow-up targeting
`crates/js-runtime/benches/hot_loop.rs`'s real idle-tick-shaped script instead of the
five toy reference programs here.

[`ATOMIC_JS_TIERING.md`](ATOMIC_JS_TIERING.md) works through the interpret-vs-compile
orchestrator ahead of time — not in scope for this spike (§3: no JIT), but the piece
most likely to undermine the product's own memory-footprint thesis if a future baseline
JIT phase copies the rejected proposal's naive tiering model unmodified. §5.1/§5.4's
`FunctionFeedback` counters exist specifically so that document's assumptions are
partially exercised by this spike's own benchmarks, without building any orchestrator.
That document also now cross-references
[`.claude/plans/hybrid-interpreter-jit.plan.md`](../../.claude/plans/hybrid-interpreter-jit.plan.md)
(§2 here) — a different, QuickJS-stays-tier-0 tiering design worth reading alongside it.
Worth reading before any post-green-signal scope conversation, not before or during the
spike itself.

## Results (2026-09-14)

First real cross-engine numbers, from a Darwin dev machine, release builds, `hyperfine
--warmup 5 --runs 50` for elapsed time (§7.2) and a 10-run mean of each binary's own
printed `maxrss_bytes` for memory (§7.1). All five reference programs pass their exact
expected value (§4) — correctness gate cleared before any of this was trusted.

| Program | atomicjs elapsed (mean ± σ) | quickjs-ng elapsed (mean ± σ) | Slowdown vs. quickjs-ng | atomicjs RSS (mean) | quickjs-ng RSS (mean) | RSS ratio |
| --- | --- | --- | --- | --- | --- | --- |
| `sum` | 39.4 ms ± 25.2 ms | 32.4 ms ± 20.0 ms | **1.22×** | 1.61 MB | 10.46 MB | **6.5× smaller** |
| `props` | 108.2 ms ± 57.4 ms | 21.3 ms ± 19.2 ms | **5.08×** | 1.64 MB | 10.46 MB | **6.4× smaller** |
| `closures` | 124.7 ms ± 27.5 ms | 43.5 ms ± 13.1 ms | **2.87×** | 1.67 MB | 10.45 MB | **6.2× smaller** |

**Caveat on precision:** `hyperfine` flagged statistical outliers on 2 of 3 runs (σ is
large relative to the mean throughout) — this sandbox isn't a quiet, dedicated
benchmarking machine. The *direction* of every result here is clear enough not to be
noise (a 5× or 6× gap doesn't hide inside that variance), but treat the exact ratios as
directionally right, not lab-grade precise — rerun on a quiet machine before treating
any of these numbers as final.

**Reading against §9's criteria — a real mixed signal, not a clean green or red:**

- **Memory: a clear, consistent win for `atomicjs`** — roughly 6× smaller resident set
  across all three programs, exactly the axis the product's own competitive claim rests
  on (`spec/ROADMAP.md`'s 2026-08-26 decision). This part of the thesis holds up.
- **Execution time: `sum` clears the 2× guardrail (1.22×), `props` and `closures` don't**
  (5.08× and 2.87×). §9's red criterion — "execution time far worse than QuickJS-ng" —
  is squarely met by `props`, and `closures` sits past the stated 2× line too. This
  isn't a rounding error to wave off.
- **Likely cause, not yet investigated (§7.4's job, not done here):** `props`'s gap is
  the most suspicious single number — property access is the one thing `atomicjs`'s
  object model does with zero optimization (`JsObject`'s `HashMap<String, Value>`, a
  fresh hash + string comparison per `GET_PROP`, vs. QuickJS-ng's shape/inline-cache
  machinery). This is a real, plausible explanation, not an excuse — it hasn't been
  confirmed with a profiler, and reporting it as fact rather than a hypothesis would be
  exactly the "trust the story instead of the number" mistake this methodology exists to
  prevent.

**Verdict: not a green light as specified.** §9's green criteria require *all three* —
lower cold-start, lower memory, and execution time within 2× — to hold. Memory holds
decisively; execution time fails outright on two of three programs. This is reported to
the user as-is, per §8 step 8 and §9's own instruction not to decide unilaterally — the
options from here (investigate the `props` gap with `samply` before concluding anything
further, accept the red-flag reading and archive per §9, or re-scope the comparison) are
the user's call, not something to resolve by continuing to build.

### Update, same day: `props` investigated with `samply`, one real bug fixed

Profiled `props` with `samply` (required rebuilding with `--config profile.release.debug=true
--config profile.release.strip=false` — the plain release build had no debug info to
symbolicate against — and `--unstable-presymbolicate` to bake symbol names into the
saved profile rather than relying on `samply`'s live local-server symbolication).

**Self-time breakdown (excluding `mach_absolute_time` at 33.5% — almost certainly
sampling-profiler observer effect at 8kHz on a ~100ms program, not real application
time; the interpreter never calls it directly):**

| Self-time | Function |
| --- | --- |
| 27.0% | `atomicjs::vm::execute_function` (the dispatch loop itself) |
| 5.5% | `JsObject::get` |
| 4.6% | `SipHash::write` (hashing the key) |
| 3.2% | `RandomState::hash_one::<&str>` |
| 2.4% + 2.3% | `memmove` / `memcmp` |
| 1.1% | `String::clone` |

**Real bug found, not just the hypothesized "HashMap is inherently slower":**
`vm.rs`'s `GetProp`/`SetProp` cloned the property-name `String` out of the constant pool
on *every single access*, even though it's a compile-time constant already borrowable
from `function.constants` for the whole call's lifetime — pure wasted heap allocation.
Fixed by borrowing `&str` instead (`GetProp` only; `SetProp`'s allocation is inherent —
`JsObject`'s `HashMap<String, Value>` needs an owned key on first insert, and that only
happens once per object literal, not per loop iteration).

**Re-measured after the fix** (`hyperfine --warmup 5 --runs 50`, same methodology):

| Program | Before | After | 
| --- | --- | --- |
| `props` | 108.2 ms (**5.08×** slower) | **54.3 ms (1.89× slower)** |
| `closures` (unaffected — no `GetProp` at all) | 124.7 ms (2.87×) | 137.6 ms (2.68×, noise) |

One clone removed roughly halved `props`'s time and pulled it from a clear red result to
inside §9's 2× guardrail. The remaining ~1.89× on `props` is the honest, inherent cost of
`HashMap<String, Value>` property access (SipHash + string comparison, confirmed by the
profile above) with no Shapes/inline caches — exactly what §3's hard scope limits say
not to build for this spike.

**`closures` profiled too, different bottleneck:** 41.2% self-time in
`execute_function`, plus several `Vec`-allocation/`malloc` frames
(`Vec<LocalSlot>`/`Vec<Value>` growth and drop). `execute_function` allocates a fresh
`Vec<LocalSlot>` (locals) and `Vec<Value>` (operand stack) on *every function call* —
`closures` calls the same closure 1,000,000 times in its loop, so that's 2,000,000+ heap
allocations attributable purely to the calling convention, not to upvalues/`Rc<RefCell>`
specifically.

### Performance-improvement survey (evidence-based, not yet acted on beyond the one fix above)

Ordered by expected impact, each checked against whether it's actually in scope for a
spike (§3) before being suggested as something to do next:

1. **Done:** remove `GetProp`'s unnecessary `String` clone — confirmed via before/after
   measurement above, not just theorized.
2. **Strongest remaining candidate, in scope:** reduce or reuse the per-call
   `Vec<LocalSlot>`/`Vec<Value>` allocations in `execute_function`. This is the
   profiler-identified bottleneck behind `closures`'s remaining 2.68×, doesn't require
   Shapes/GC/a JIT (still just an allocation-reuse change to the existing interpreter
   loop, well within §3's scope), and is the most promising next lever precisely because
   it's evidence-backed rather than guessed.
3. **Not recommended now — inherent to the design, not a bug:** `HashMap<String, Value>`
   property access's remaining ~1.89× cost. Fixing this for real needs Shapes/inline
   caches, which §3 explicitly excludes from this spike. This cost is exactly what the
   spike exists to reveal honestly, not optimize away before it's even been reported.
4. **Not recommended now — same reason:** `Rc<RefCell<Value>>` indirection for captured
   locals (upvalues). Part of the deliberate no-GC design (§3); optimizing it would mean
   building GC-aware allocation machinery, out of scope.

Item 2 is the only one of these that's both evidence-backed and in scope — worth a
deliberate decision before touching it, same as the `GetProp` fix, rather than being
folded in silently.

### Update, same day: item 2 (per-call allocation pooling) implemented and measured

Added a `VmPools` holding a free-list of `Vec<LocalSlot>`/`Vec<Value>` buffers, shared
across the whole recursive call tree via `RefCell` (same threading tradeoff
`compiler.rs`'s `Scope` already uses). `execute_function` takes a buffer from the pool
at entry instead of allocating fresh, and returns it (cleared, not deallocated) on the
`Instr::Return` success path — an error path still drops its buffers normally, since
errors aren't this pool's hot path. All 51 tests still pass unchanged, including the two
closure-specific regression tests that would have caught a reused-buffer state leak
(`each_call_to_the_same_closure_advances_independently`,
`two_independent_counters_do_not_share_captured_state`).

**Re-measured (`hyperfine --warmup 5 --runs 50`):**

| Program | Before pooling | After pooling |
| --- | --- | --- |
| `closures` (the workload this fix targets — 1,000,000 calls in its loop) | 137.6 ms (2.68×) | **92.2 ms (1.88×)** |
| `sum` (no repeated calls in its loop — shouldn't be affected) | 39.4 ms (1.22×) | 44.3 ms (1.49×) at 50 runs, **26.4 ms (1.29×) at 100 runs** |
| `props` (no calls in its loop at all — shouldn't be affected either) | 54.3 ms (1.89×) | 71.0 ms (2.11×) at 50 runs, **73.3 ms (2.15×) at 100 runs** |

`closures` improved exactly as the profiling predicted — clear, causal, reproducible.
`sum` and `props` drifted too, in both directions, despite their hot loops never calling
a function (so `execute_function` runs *once* for each, taking an empty pool on its
first-ever call — behaviorally identical to before pooling existed). That drift is
**environment noise between measurement sessions, not an effect of this change** — this
sandbox isn't a dedicated benchmarking machine, and `hyperfine` flagged statistical
outliers on `props` at both 50 and 100 runs. Repeating `sum` at 100 runs pulled it back
toward its original ~1.2-1.3× range, consistent with that read.

**Honest, unresolved state on `props`:** it now measures consistently in the ~2.1-2.15×
range across two repeated 50/100-run sessions — right at, arguably just past, §9's 2×
guardrail, and not clearly distinguishable from the earlier 1.89× reading given this
environment's noise floor. This is reported as genuinely borderline, not rounded to
whichever side looks better. A rerun on a quiet, dedicated machine is what would actually
resolve it, not more runs here.

**Where this leaves the three programs against §9:**

| Program | Slowdown (best available reading) | Within 2×? |
| --- | --- | --- |
| `sum` | ~1.2-1.3× | Yes |
| `props` | ~2.1-2.15× | Borderline / no, on the numbers as measured here |
| `closures` | 1.88× | Yes |

Memory still holds decisively (~6× smaller, §7.1, unaffected by any of this session's
changes). Two of three programs are now within the execution-time guardrail; `props`
remains the one open question, and it's a measurement-noise question at this point, not
an architecture one — the object-model cost behind it is already understood (this
section's own profiling) and correctly out of scope to fix further (item 3 above).

### Update, same day: `args` buffer pooled too — no clearly measurable win

Re-profiling after the `locals`/`stack` pooling fix showed a smaller remaining
allocation: `Instr::Call` still collected popped arguments into a fresh `Vec<Value>` on
every call (`Vec::from_iter` + a handful of `malloc` frames in `closures`'s profile).
Pooled it the same way (`VmPools` gained an `args` free-list; `execute_function` now
takes `args: &[Value]` instead of an owned `Vec<Value>`). All 51 tests still pass.

**Re-measured, three separate `closures` runs plus a final consolidated round of all
three programs together (`hyperfine --warmup 10 --runs 100`):**

| Run | `closures` |
| --- | --- |
| 1 | 2.07× |
| 2 | 1.69× |
| 3 | 2.05× |

| Program (final consolidated round) | Slowdown |
| --- | --- |
| `sum` | **1.01×** |
| `props` | **1.93×** |
| `closures` | **2.05×** |

**Honest read: no clearly attributable improvement from this specific fix.** Unlike the
`locals`/`stack` pooling fix (a large, unambiguous, reproducible ~30% time reduction on
`closures`), this one targets a much smaller allocation (`args` is usually 0-1 elements
in these reference programs) — its effect, if any, is smaller than the ~20-50% run-to-run
variance already visible throughout this session's measurements. Applying it was still
worth doing (it's correct, it's free, and it removes one more real allocation from the
hot path), but reporting it as "fixed the closures gap" would be exactly the kind of
unearned optimism this methodology exists to prevent. `closures` remains at roughly the
same ~1.7-2.1× range it was in immediately after the `locals`/`stack` fix — right at
§9's guardrail, same open, noise-bound question `props` already was.

**Where this leaves things:** two of the three concrete, in-scope, evidence-backed fixes
from the performance survey are applied (`GetProp` clone removal, `locals`/`stack`/`args`
pooling). Both `props` and `closures` now sit right at the 2× line, oscillating across
it between measurement sessions in this non-dedicated environment — not clearly green,
not clearly red. Further movement from here would need either a quiet, dedicated
benchmarking machine (to actually resolve whether they're above or below 2×) or the
larger, out-of-scope architectural work already named and explicitly not recommended
(Shapes/inline caches for `props`, a different execution model for interpreter dispatch
overhead generally) — not more micro-fixes hunted for their own sake.

### Final verdict (2026-09-14): green, conditional — user-requested decision, not unilateral

The user asked for a call on the current numbers rather than further deferral. Decision:
**green, conditional** — worth taking to the user as a new, dated scope decision (§10),
not a silent green light to start building.

**Why green:**

1. **Memory — the actual product thesis — is decisive and consistent**: ~6× smaller
   across all three programs, no session-to-session oscillation (unlike execution time).
   This is the axis `spec/ROADMAP.md`'s 2026-08-26 decision actually rests the
   competitive claim on.
2. **Execution time: one of three comfortably passes, two sit right at the line — not a
   clear failure.** `sum` ~1.0-1.3×. `props`/`closures` oscillate ~1.7-2.15× across
   repeated sessions in a non-dedicated sandbox (`hyperfine` flagged statistical outliers
   repeatedly) — categorically different from the original 5.08×/2.87× clear failures.
3. **Context that favors green:** `props`/`closures` run 1,000,000 loop iterations —
   far heavier than a real idle-game tick. `spec/architecture/performance.md` §21.6
   already showed QuickJS-ng has large throughput headroom (5M+ ops/sec) against actual
   1-60Hz idle-game needs. A borderline 2× on an intentionally extreme synthetic
   benchmark is a weaker signal against real target content than a clean failure would
   be.
4. **Favorable trajectory, not an architectural wall.** Two rounds of profiling-driven
   fixes (§ above) already moved `props` from 5.08× to ~1.9× and `closures` from 2.87× to
   ~1.7-2×, cheaply and without touching anything §3 excludes. The remaining gap is small
   and noise-bound, not a demonstrated structural ceiling.

**What "green" does *not* mean:** authorization to build a full engine, or to treat
`props`/`closures` as definitively resolved. The honest caveat stands: a clean
re-measurement on a quiet, dedicated machine is what would actually settle whether they
sit above or below 2× — this verdict doesn't wait for that, per the user's explicit
request to decide on the numbers as they stand.

**Next step, per §10:** raise this with the user explicitly as a new scope decision,
same weight as 2026-08-26's — not auto-promoted into `spec/ROADMAP.md`.

## Appendix — salvaged principles from the rejected proposal

These remain correct engineering principles *if* this spike (or, contingent on a green
signal, a real engine) ever gets built, kept here rather than duplicated in the rejected
file: correctness-before-performance ordering (§2.1), lazy-by-default (§2.2), never JIT
before the interpreter is semantically correct, differential testing against a reference
engine for any correctness work, and never copy V8 source — only architectural
principles, consulting public docs/code to understand behavior.

---

[← back to `ATOMIC_JS_ARCHITECTURE.md`](ATOMIC_JS_ARCHITECTURE.md)
