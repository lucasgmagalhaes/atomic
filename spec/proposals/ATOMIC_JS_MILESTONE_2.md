# ATOMIC_JS_MILESTONE_2.md — Next Bounded Milestone: Run `hot_loop.rs`'s Real Idle-Tick Shape

## 0. Status

Not scheduled work yet — a draft scope for the next milestone, per the user's explicit
request after [`ATOMIC_JS_SPIKE.md`](ATOMIC_JS_SPIKE.md)'s final verdict ("green,
conditional," 2026-09-14). No deadline (the user's own instruction) — the boundary here
is **scope, not time**: this milestone is done when its one reference workload runs
correctly and is benchmarked, not when a clock runs out. Don't add anything beyond what
this document lists, even if it looks trivial — that's exactly how the spike stayed
small enough to finish.

Still governed by `ATOMIC_JS_SPIKE.md`'s own rules unless explicitly superseded below:
still `crates/atomicjs`, still outside the root workspace, still no production
dependents, still correctness-before-performance, still small commits with tests.

## 1. The actual question being tested

Not "can AtomicJS run arbitrary real JavaScript" — that's unbounded and not this
milestone's job. The narrow question:

> Can AtomicJS correctly run the exact same idle-game-shaped script this codebase
> already uses to reason about JS engine sufficiency
> (`crates/js-runtime/benches/hot_loop.rs`'s `HOT_LOOP_JS`), and how does it compare to
> QuickJS-ng's already-measured number for that same script?

This reuses an existing, agreed-upon reference workload instead of inventing a new one
or chasing an unbounded "real idle game" — `spec/architecture/performance.md` §21.6
already measured QuickJS-ng at **~185ms for 200,000 ticks** (5M+ transcendental-shaped
ops/sec). That's the number this milestone compares against; no new QuickJS-ng
benchmarking needed.

## 2. The reference workload (verbatim, not paraphrased)

```javascript
(function () {
    let gold = 1.0;
    let gems = 1.0;
    let energy = 1.0;
    let goldMult = 1.0001;
    let gemsMult = 1.00005;
    let energyMult = 1.00002;
    let ticks = 200000;
    for (let i = 0; i < ticks; i++) {
        gold = gold * goldMult + Math.sqrt(i % 997 + 1);
        gems = gems * gemsMult + (gold % 13);
        energy = energy * energyMult + Math.log(gems + 1);
        if (gold > 1e12) { gold = gold / 1e6; goldMult *= 1.0000001; }
        if (gems > 1e12) { gems = gems / 1e6; gemsMult *= 1.0000001; }
        if (energy > 1e12) { energy = energy / 1e6; energyMult *= 1.0000001; }
    }
    return gold + gems + energy;
})()
```

**Expected value:** not hand-computed here (200,000 floating-point iterations with
conditional resets isn't something to eyeball correctly) — the ground truth is
whatever `crates/js-runtime/examples/quickjs-run.rs`-style `Context::eval` already
returns for this exact script on this machine. Capture that value once, first, and use
it as the golden value AtomicJS's own run must match — same "correct, then measured"
discipline as `ATOMIC_JS_SPIKE.md` §4/§8.

## 3. Grammar/feature gap — exactly what this script needs that AtomicJS doesn't have yet

Checked character-by-character against the script above, not guessed:

| Feature | Currently in AtomicJS? | Needed because |
| --- | --- | --- |
| `*`, `/`, `%` binary operators | No (only `+`, `<`) | `gold * goldMult`, `i % 997`, `gold / 1e6` |
| `>` comparison | No (only `<`) | `if (gold > 1e12)` |
| Multiplicative precedence (`*`/`/`/`%` bind tighter than `+`) | N/A, doesn't exist yet | `Math.sqrt(i % 997 + 1)` must parse as `sqrt((i % 997) + 1)` |
| `*=` compound assignment | No (only `+=`, hardcoded to `Add`) | `goldMult *= 1.0000001` |
| Scientific-notation numeric literals (`1e12`, `1e6`) | No (`ATOMIC_JS_SPIKE.md` §5.2 explicitly excluded this) | `1e12`, `1e6` |
| `if (cond) { ... }` (no `else`) | No | three `if` statements, none with an `else` branch |
| `Math.sqrt(x)`, `Math.log(x)` | No (no native/builtin function concept at all) | both called every tick |
| IIFE — `(function () { ... })()` | **Already works, unverified** — `parse_primary`'s `(expr)` case plus `parse_call_or_member`'s trailing-`(args)` loop should already compose correctly; this milestone is the first real exercise of it | the whole script is one |
| Plain `=` assignment | **Already implemented, unverified** — `Expr::Assign` exists in the AST/compiler/VM but no reference program has exercised it yet | `gold = gold / 1e6` etc. |

Two rows above are marked "already works, unverified" deliberately — don't assume they're
fine just because the code exists; this milestone is the first real test of both.

## 4. Scope limits (the actual boundary, since there's no time-box)

- **No arrays, no strings beyond property/method names, no `while`/`switch`/`class`/
  `try`.** Not used by this one script.
- **No `else` branch on `if`.** Not used here — add only if a later reference workload
  needs it, not preemptively (same discipline `ATOMIC_JS_SPIKE.md` §5.2 already used for
  its own grammar).
- **No general native-function/builtin-object system.** `Math.sqrt`/`Math.log` are
  recognized as two specific, hardcoded call patterns (compiler pattern-matches a call
  whose callee is `Member(Identifier("Math"), "sqrt"|"log")`) rather than building a real
  global-object-with-native-methods model. A proper `Math` object, or any other builtin,
  is explicitly out of scope until a later reference workload needs a *different* native
  function — building the general mechanism now, for exactly two functions, would be the
  premature-generality mistake `spec/RULES.md`'s YAGNI guidance already warns against.
- **`-` (subtraction) and unary minus: not needed by this script, not added.** Check
  before assuming — this script never subtracts or negates.
- **`==`/`!=`/`<=`/`>=`/`&&`/`||`/`!`: not needed, not added.** Only `>` joins `<`.
- **Still no GC, no JIT, no error/exception model, no dependents** — everything
  `ATOMIC_JS_SPIKE.md` §3 already ruled out stays ruled out.

## 5. Concrete implementation additions

### 5.1 Lexer (`lexer.rs`)

- Scientific notation: extend the number-scanning branch to also accept an `e`/`E`
  followed by an optional `+`/`-` and digits (`1e12`, `1e-6` if it ever comes up, though
  this script only uses positive exponents).
- New single/double-char tokens: `Star` (`*`), `Slash` (`/`), `Percent` (`%`), `Greater`
  (`>`), `StarAssign` (`*=`).

### 5.2 AST (`ast.rs`)

- `BinOp` gains `Mul`, `Div`, `Mod`. `Greater` does **not** need its own variant — compile
  `a > b` as `Lt` with swapped operands (`b < a`), reusing the existing opcode instead of
  adding one. Document this reuse where it's implemented, same as the postfix-increment
  reuse trick already documented in `bytecode.rs`.
- `Expr::CompoundAssign` gains an `op: BinOp` field (currently hardcoded to `Add` in the
  compiler) so `*=` (and the existing `+=`) share one AST shape.
- `Stmt::If { cond: Expr, then_branch: Vec<Stmt> }` — no `else` field (§4).

### 5.3 Parser (`parser.rs`)

- New precedence level between `parse_additive` and `parse_unary`: `parse_multiplicative`
  handling `*`/`/`/`%`, left-associative, same shape as `parse_additive`'s own loop.
- `parse_relational` also checks `Token::Greater`, compiling to `Expr::Binary { op:
  BinOp::Less, left: <swapped>, right: <swapped> }` (the reuse from §5.2).
- `parse_assignment` also checks `Token::StarAssign`, producing `CompoundAssign { op:
  BinOp::Mul, .. }` (generalizing the existing single-operator check).
- `parse_statement` gains an `if` branch: `if (` expr `)` block — reuses the same
  brace-block parsing already used by `for`/function bodies.

### 5.4 Bytecode (`bytecode.rs`) / compiler (`compiler.rs`)

- New opcodes: `Mul`, `Div`, `Mod`.
- `Instr::JumpIfFalse` already exists and is exactly what `if` compiles to (`compile(cond)
  → JumpIfFalse(after_then) → compile(then_branch) → after_then:`) — no new control-flow
  opcode needed, same mechanism `for`'s condition already uses.
- New opcode: `CallNative(NativeFn)` where `NativeFn` is a two-variant enum (`Sqrt`,
  `Log`) — the compiler emits this when it recognizes the `Math.sqrt`/`Math.log` call
  pattern (§4) instead of the normal `GetProp` + `Call` sequence a real method call would
  use. Document this as the narrow, hardcoded special case it is.

### 5.5 VM (`vm.rs`)

- `Mul`/`Div`/`Mod`: straightforward `f64` operations via the existing `as_number` helper.
- `CallNative(NativeFn::Sqrt | NativeFn::Log)`: pop one operand, apply `f64::sqrt`/
  `f64::ln` (confirm `Math.log` is natural log, not `log10`, against real JS semantics
  before assuming), push the result.

## 6. Methodology (reusing `ATOMIC_JS_SPIKE.md`'s, not reinventing)

- Correctness gate first: assert AtomicJS's result for `HOT_LOOP_JS` matches the
  QuickJS-ng golden value from §2, exactly — not "close enough." Floating-point `sqrt`/
  `ln` should agree bit-for-bit between the two engines on the same platform (both
  ultimately call the same libm), so an exact match is the right bar, not a tolerance.
- Same benchmark harness as the spike (§7 there): a `hot_loop` entry added to both
  comparison binaries' `script_for` match, `hyperfine --warmup 5 --runs 50`.
- Same honesty standard on noise: this sandbox isn't a dedicated benchmarking machine
  (`ATOMIC_JS_SPIKE.md`'s Results section already documented this extensively) — report
  ranges across repeated sessions, don't cherry-pick the flattering number.
- Append results to this file's own `## Results` section once run, same convention.

## 7. Go / no-go

Reuse `ATOMIC_JS_SPIKE.md` §9's shape, applied to this one additional workload:

- **Green:** AtomicJS matches QuickJS-ng's result exactly, and its elapsed time for
  200,000 ticks isn't worse than roughly 2× QuickJS-ng's ~185ms baseline (i.e., stays
  under ~370ms) — consistent with the threshold already used for `sum`/`props`/
  `closures`, not a new bar invented for this milestone.
- **Red:** wrong result (a real correctness bug, not just slow), or execution time far
  past that guardrail on a workload this much closer to what `spec/architecture/
  performance.md` §21.5 actually describes as representative.
- Either way: report to the user, same as before — this document doesn't pre-authorize
  what happens next any more than the spike's own verdict did.

## 8. What this milestone deliberately does not decide

Whether AtomicJS ever gets wired into `profile-worker`/`js-runtime` for real. That's a
larger, separate decision (`ATOMIC_JS_SPIKE.md` §10) this milestone doesn't bring any
closer on its own — it only answers whether AtomicJS can correctly and competitively run
one additional, more representative reference workload than the original five.

---

[← back to `ATOMIC_JS_SPIKE.md`](ATOMIC_JS_SPIKE.md)
