# ATOMIC_JS_MILESTONE_3.md — Next Bounded Milestone: Subtraction + a Multi-Function Call Graph

## 0. Status

Completed 2026-09-14, per the user's request to scope the next milestone after
[`ATOMIC_JS_MILESTONE_2.md`](ATOMIC_JS_MILESTONE_2.md) (hot-loop workload, completed,
green-conditional). No deadline — same discipline as Milestone 2: the boundary is
**scope, not time**.

Still governed by `ATOMIC_JS_SPIKE.md`'s own rules unless explicitly superseded below:
still `crates/atomicjs`, still outside the root workspace, still no production
dependents, still correctness-before-performance, still small commits with tests.

## 1. The actual question being tested

Every reference program so far (`sum`/`obj`/`closure`/`props`/`closures`/`hot_loop`) is
either a flat loop or a single self-contained function/closure. None of them:

- use subtraction or unary minus at all (`ATOMIC_JS_MILESTONE_2.md` §4 explicitly
  deferred `-` because `hot_loop`'s script doesn't use it),
- have one named function call a *different* named function (only `hot_loop`'s
  `Math.sqrt`/`Math.log` and the closures' self-calls exist as "call something else"
  cases, and those are hardcoded native ops / a returned closure, not two independent
  user-defined functions),
- exercise recursion.

Two of those three are confirmed gaps by direct testing (not guessed): a scratch
`fact`-style script hit `unexpected character: '-'` in the lexer immediately — `-` truly
does not exist anywhere in AtomicJS yet, not even unary. A second scratch script
(`countdown(n)` calling itself via `n / 2`, avoiding `-` entirely) ran and returned the
correct value on the first try — **recursion of a named function already works**,
untested until now but not actually broken. This milestone's real job is narrower than
it first looked: add subtraction, and add a *second* reference workload that exercises
both recursion and a two-function call graph together, since neither has a regression
test pinning it yet.

## 2. The reference workload (new — no existing script in this repo covers this shape)

Unlike Milestone 2, there's no existing agreed-upon script elsewhere in the repo to
reuse (`hot_loop.rs` was the only other one, already spent). This one is new, so it must
be small, deterministic, and justified by the gap above rather than invented for its own
sake — an idle-game "upgrade cost curve" shape, which is exactly the domain this engine
exists for:

```javascript
function cost(level) {
    if (level < 1) { return 10; }
    return cost(level - 1) * 1.15;
}
function affordableLevels(budget, level) {
    let price = cost(level);
    if (price > budget) { return level; }
    return affordableLevels(budget - price, level + 1);
}
affordableLevels(1000, 0);
```

`cost` recurses on itself; `affordableLevels` recurses on itself *and* calls a different
named function (`cost`) — covering both open questions in one script without inventing
unrelated new surface area.

**Expected value:** captured from the real QuickJS-ng engine already embedded in this
repo (`crates/js-runtime`), not hand-derived — `19`. Confirmed by direct execution
during this drafting pass, same "capture the golden value first" discipline as
`ATOMIC_JS_SPIKE.md` §4 / `ATOMIC_JS_MILESTONE_2.md` §2.

## 3. Grammar/feature gap

| Feature | Currently in AtomicJS? | Needed because |
| --- | --- | --- |
| Binary `-` | No | `level - 1`, `budget - price` |
| Unary `-` | No | not used by this script, but binary `-`'s lexer/parser work makes it nearly free — see §4 on whether to include it anyway |
| Two independent named functions, one calling the other | Untested (only self-recursion and closure self-calls exist as prior art) | `affordableLevels` calls `cost` |
| Recursion (a function calling itself by name) | **Already works, confirmed by direct test** — no code change needed, only a regression test | both `cost` and `affordableLevels` recurse |

## 4. Scope limits

- **Binary `-` only, not unary `-`.** The reference script never negates a value (only
  subtracts two positives, both of which stay non-negative in this script's domain).
  Adding unary minus now, unused, would repeat the exact premature-generality mistake
  `ATOMIC_JS_MILESTONE_2.md` §4 already declined for `else`-branches and `!=`/`<=`/etc. —
  wait for a reference workload that actually negates something.
- **No mutual recursion beyond two functions, no forward references** (`cost` is
  declared before `affordableLevels` uses it, and neither is called before its own
  declaration is compiled) — not exercised by this script, not added speculatively.
- **No default/optional parameters, no varargs, no `arguments` object.** Not used here.
- **Still no GC, no JIT, no error/exception model, no dependents** — everything
  `ATOMIC_JS_SPIKE.md` §3 already ruled out stays ruled out.

## 5. Concrete implementation additions

### 5.1 Lexer (`lexer.rs`)

- New token: `Minus` (`-`). No `-=`/`--` — not used by this script, same reasoning as
  §4.

### 5.2 AST (`ast.rs`)

- `BinOp` gains `Sub`.

### 5.3 Parser (`parser.rs`)

- `parse_additive` also checks `Token::Minus`, same shape as its existing `Plus`
  handling (left-associative, same precedence level — `-` binds exactly like `+`, not
  looser or tighter).

### 5.4 Bytecode (`bytecode.rs`) / compiler (`compiler.rs`)

- New opcode: `Sub`.

### 5.5 VM (`vm.rs`)

- `Sub`: straightforward `f64` subtraction via the existing `as_number` helper, same
  shape as `Add`/`Mul`/`Div`/`Mod`.

### 5.6 Tests

- New integration test pinning the §2 script's exact golden value (`19`), added to
  `tests/common/mod.rs` alongside the existing named fixtures and asserted in
  `tests/vm_test.rs`, same convention as `HOT_LOOP`.
- A second, smaller test isolating plain two-function calling (no recursion) to keep
  the two gaps in §1 independently diagnosable if one regresses later — e.g. a
  three-line `add(a, b)` called from `main()`-shaped script, not the full recursive one.

## 6. Methodology

Same as `ATOMIC_JS_MILESTONE_2.md` §6 — exact-match correctness gate against the
QuickJS-ng golden value, no execution-time benchmarking required for this milestone
(the script is small and non-hot-loop-shaped; it's testing call-graph correctness, not
performance). If a future milestone wants a hot-loop-shaped version of this same
call-graph shape, that's a separate, later decision — don't fold it in here.

## 7. Go / no-go

- **Green:** AtomicJS's result for the §2 script matches QuickJS-ng's `19` exactly, and
  the two new regression tests (recursion+cross-function-call, and plain
  cross-function-call) both pass.
- **Red:** a real correctness bug in subtraction, function resolution, or recursion —
  not applicable to performance, since none is being measured here.
- Either way: report to the user — this document doesn't pre-authorize what happens
  next any more than the spike's or Milestone 2's own verdicts did.

## 8. Results

Implemented exactly within §4's limits:

- `-` is a binary additive-precedence token/operator only; no unary minus, `-=`, or
  decrement was added.
- `BinOp::Sub` lowers to `Instr::Sub`, which performs the same `f64` stack operation
  shape as the other arithmetic instructions.
- The full recursive upgrade-cost workload is an AtomicJS integration fixture and
  returns the QuickJS-ng golden value `19` exactly.
- A separate test verifies a named function can call a different named function without
  recursion (`add` from `doubleSum`), so a regression is diagnosable independently of
  the recursive workload.

`cargo test --manifest-path crates/atomicjs/Cargo.toml` passed all 59 tests. No runtime
benchmark was run or claimed: this milestone is a call-graph correctness gate, not a
hot-loop performance comparison, as specified in §6.

---

[← back to `ATOMIC_JS_SPIKE.md`](ATOMIC_JS_SPIKE.md) · [← `ATOMIC_JS_MILESTONE_2.md`](ATOMIC_JS_MILESTONE_2.md)
