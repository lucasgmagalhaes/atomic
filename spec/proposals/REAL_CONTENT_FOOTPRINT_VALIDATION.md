# REAL_CONTENT_FOOTPRINT_VALIDATION.md — Validating the Memory Thesis on Real Content

## 0. Status

Not a new engine decision — this validates a decision already made
(`spec/ROADMAP.md`'s 2026-08-26 product-scope decision: many isolated profiles at low
memory beats Chromium-based multi-account tools). Everything measured so far
(`crates/js-runtime/benches/hot_loop.rs`, `xtask bench-footprint`) is synthetic — pure JS
with no DOM, or a blank test page. Neither is "real content," and neither has ever been
compared against a real competitor. This plan closes both gaps. Nothing here is
scheduled work until run; results, once produced, should be appended to
`spec/architecture/performance.md`'s §21.6-style dated results section, the same
convention the existing benchmarks already use.

Also answers a direct question raised alongside this plan: **should Atomic embed
Chromium/CEF as an automatic fallback engine, with logic deciding when to trigger it?**
§5 gives a direct recommendation — no, not that shape — before the rest of this plan,
since the answer changes what's worth measuring here.

## 1. What's actually being validated

`spec/ROADMAP.md`'s product-scope decision rests on one claim: this engine's per-profile
memory/CPU/cold-start footprint beats a Chromium-based tool's, at the scale of many
concurrently open profiles. The only numbers that exist today:

- `hot_loop.rs`: QuickJS-ng throughput on a synthetic tick handler, **no DOM at all**.
  Answers a different question (JIT necessity), not this one.
- `xtask bench-footprint`: real `profile-worker` processes against a *blank* test page
  (`<html><body><h1>xtask bench-footprint page</h1></body></html>`), sampled **once**,
  ~1.2s after spawn. This measures engine/process *baseline* overhead — not a real
  page's DOM size, CSS rule count, JS heap growth, timers, or a long-running session's
  memory trend. It also has no comparator: **nobody has measured what a real
  Chromium-based instance costs for the same page**, so "~108 MiB/profile" has never
  been checked against the thing it claims to beat.

This plan has three phases, each gated on the previous one producing a sane result —
don't skip ahead on an unverified number, same discipline `ATOMIC_JS_SPIKE.md` uses.

## 2. Phase A — build the already-specified Idle Game Benchmark

`spec/architecture/performance.md` §21.5 already describes this benchmark; it has never
been implemented. This phase just builds it.

### 2.1 The test page (concrete, not "something like the example")

A static asset, `xtask/assets/idle_game_bench.html`, served by `xtask`'s existing tiny
HTTP server (`spawn_test_server` in `xtask/src/main.rs`, extended to serve file content
instead of the current fixed `PAGE_HTML` literal — small change, same mechanism):

- 1,000 `<div>` elements in a grid, each showing a counter text node.
- A single `setInterval(..., 50)` updates all 1,000 counters' `textContent` and toggles
  one of 3 CSS classes on each (cycling) — matches §21.5's "1,000 counters / updates
  every 50ms / frequent text mutations / periodic class changes" directly.
- One `requestAnimationFrame` loop moving a single element (a minimal stand-in for
  "animation loop") — cheap to add, exercises a different code path than the interval.
- Moderate, fixed DOM size and a small real stylesheet (not inline styles only) — CSS
  rule matching should be exercised, not bypassed.

### 2.2 Correctness gate, before any number is trusted

Given `spec/INDEX.md`'s current completion numbers (DOM 38%, CSS/layout 48%, browser
APIs 35%), **confirm this page actually behaves correctly first** — timers firing on
schedule, class/text mutations visibly applying, `requestAnimationFrame` actually
looping — before trusting any memory number it produces. A number from content that's
silently broken (a timer that stopped firing, a mutation that didn't apply) isn't a
representative measurement, it's a measurement of a bug. This is a manual/visual check
(`apps/shell`'s existing debug/inspection tooling), not a new automated test suite —
proportionate to what this phase needs.

### 2.3 Sampling: repeated, not a single before/after pair

`xtask bench-footprint`'s current shape (`measure()` in `xtask/src/main.rs`) samples
once, 400ms after an 800ms warm-up. That can't distinguish "steady low footprint" from
"footprint that's still climbing" — exactly the failure mode a long-running kiosk/idle
profile actually cares about. Extend it (new subcommand, e.g. `bench-footprint-idle`, or
a flag on the existing one) to:

- Sample every 10 seconds for a 10-minute session (configurable — these are starting
  defaults, not fixed requirements).
- Record the full series, not just first/last — report the trend (simplest: first
  sample vs. last sample delta; a full linear fit is more rigorous but not required for
  a first pass) alongside the usual instantaneous numbers.
- Run at n=1 and n=5 (matching the existing benchmark's own scale) to see whether the
  per-profile average and its growth trend both hold as concurrency increases.

## 3. Phase B — the missing comparator

Without this, "~108 MiB/profile" is a number with nothing to be compared against — the
product's whole pitch is *relative* to Chromium-based tools, and that comparison has
never actually been run.

- Serve the **exact same** `idle_game_bench.html` (§2.1) to a real, already-installed
  Chromium-based browser in headless mode (e.g. `google-chrome --headless
  --disable-gpu --user-data-dir=<fresh temp dir> <url>` — whatever browser is actually
  installed on the machine doing the comparison; no need to bundle or install a new one,
  this is a one-off measurement, not a shipped dependency).
- Sample its process RSS with the same cadence as §2.3. **Deliberately not** via
  `platform_apis::process_stats` — it's `#[cfg(windows)]`-only (confirmed by reading
  `crates/platform-apis/src/process_stats.rs:82`), unusable on this Darwin dev machine.
  Simplest portable option, matching this project's "no Node/npm anywhere, minimal
  toolchain" convention (`CLAUDE.md`'s Conventions section): shell out to `ps -o rss=
  -p <pid>` from the comparison tool — zero new dependency, works today, on this
  machine. (`sysinfo` — a small, pure-Rust, cross-platform crate — is the fallback if
  `ps`-shelling turns out to be too fragile in practice; start with the zero-dependency
  option first.)
- **This is the number that actually proves or disproves the product's stated
  competitive claim.** If Atomic's per-profile number isn't meaningfully lower than this
  real comparator's, on the same real content, the core thesis needs re-examination —
  not a bigger red flag to bury, a bigger one to report immediately.

## 4. Phase C — a real (not synthetic) idle game, contingent on A/B looking sane

If §2–3 produce a believable, favorable comparison, the strongest remaining validation
is dropping the synthetic page for an actual existing idle game (open-source, or a
snapshot of one), unmodified. This will surface real compatibility gaps the matrix
percentages already predict exist — use the resulting list to prioritize
`spec/ROADMAP.md` by what real target content actually blocks on, rather than the
abstract P0–P5 order alone. Not scoped further here — what gets prioritized depends
entirely on what phase C's specific gap list turns out to contain.

## 5. The Chromium/V8 fallback question, directly

**Recommendation: no, not as an automatic runtime fallback with detection logic.**

Reasons this doesn't fit, checked against how this project already defines itself:

1. **Weight.** CEF/Chromium is a multi-hundred-MB dependency with its own continuous
   security-patch obligation (Chromium ships CVE fixes on a near-weekly cadence). Even
   as a "rare fallback," this reintroduces the exact overhead this project's whole
   architecture exists to avoid, for however large the fallback slice turns out to be —
   and directly contradicts this workspace's own stated minimalism (`CLAUDE.md`'s
   Conventions: no Node/npm anywhere, `cargo-husky` instead of Husky, a small Cargo
   workspace).
2. **The detection problem is the hard part, not the trigger mechanism.** Reliably
   deciding "this page won't render/behave correctly here" without already having a
   general-purpose engine to diff against is an open-ended problem — subtle
   visual/behavioral bugs are far more common than outright crashes, and there's no
   cheap, reliable signal for "silently a bit wrong." Building that detector is its own
   large, unbounded research effort, arguably bigger than continuing to close the
   custom engine's own compatibility matrix.
3. **Scope-creep risk is asymmetric and bad.** If detection is imperfect (likely) or real
   target content keeps needing "just one more" unsupported feature, the fallback
   quietly becomes the common path — at which point most profiles *are* Chromium
   instances again, erasing the memory story for exactly the users who need many
   concurrent profiles. That's the single worst outcome this architecture could produce,
   and an automatic-fallback design has no natural brake against drifting into it.
4. **Two engines to maintain is a bigger commitment than one, indefinitely.** Build
   systems, security patching, and unifying the profile/security/automation model across
   both engines is a larger, more open-ended obligation than the current single-engine
   roadmap — not a one-time integration cost.

**A cheaper, honest version of the same underlying idea:**

- Skip automatic detection entirely. Make "this profile needs full web compatibility"
  an **explicit, per-profile, user-facing setting** — not a runtime trigger.
- Back that setting with the **OS's own native webview** — WebView2 (Windows, Edge/
  Chromium-backed) or WKWebView (macOS, WebKit-backed) — not a bundled Chromium/CEF
  build. The OS already ships and patches these; using one costs an API call, not a
  vendored multi-hundred-MB dependency with its own patch treadmill.
- Same shape as existing per-profile toggles already in `apps/shell`'s Settings (GPU
  adapter, FPS cap, OS-keychain — per `CLAUDE.md`'s Implementation status) — cheap to
  build, ships real value immediately, and stays opt-in rather than silently becoming
  the default path.
- This keeps the product honest about its own stated scope
  (`spec/ROADMAP.md`'s "hybrid niche, not general-purpose web browsing"): a profile that
  genuinely needs full compatibility is told plainly to use the webview-backed profile
  type, rather than the product quietly pretending to be a general browser underneath.
- Cross-platform inconsistency (WebView2 ≠ WKWebView, different engines) is an
  acceptable tradeoff here specifically because this path is explicitly the escape
  hatch, not the primary product — it doesn't need to behave identically everywhere the
  way the core engine does.

**When to actually revisit this recommendation:** if Phase C (§4) turns up so many
blocking, open-ended compatibility gaps that hardening the custom engine looks like a
multi-year tax real users won't tolerate — that's the real trigger condition for a
heavier fallback, not something to pre-build speculatively now. Same discipline as
`.claude/plans/hybrid-interpreter-jit.plan.md`'s own trigger-condition framing: build it
when a real, measured need demands it, not in anticipation of one.

## 6. Go / no-go framing

- **Phase B shows Atomic's number isn't meaningfully lower than the real comparator's,
  on the same content:** report immediately. This is a bigger red flag than anything in
  `ATOMIC_JS_SPIKE.md` — it would undermine the product's core premise, not one crate's
  fate. Don't keep re-measuring looking for a better number; bring the result to the
  user as-is.
- **Phase C shows real idle-game content mostly works, with a short, addressable gap
  list:** strong signal to keep going on the current single-engine roadmap — no fallback
  engine needed, §5's recommendation holds.
- **Phase C shows real idle-game content is broadly broken with an open-ended gap
  list:** the actual trigger to revisit §5 — because raw compatibility is the blocker,
  not because a performance number came back bad.

---

[← back to spec/INDEX.md](../INDEX.md)
