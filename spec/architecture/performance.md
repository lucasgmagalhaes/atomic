# Performance Infrastructure (Style Caching, Typed CSS, Atoms, Invalidation, Damage Tracking, Benchmarks, Profiling)

## 14. CSS and Style Optimization

Before implementing extremely complex incremental layout, prioritize style caching.

Recommended stages:

```text
Stage 1
    Selector matching cache

Stage 2
    Computed style cache

Stage 3
    Dirty style propagation

Stage 4
    Layout invalidation optimization

Stage 5
    Incremental layout

Stage 6
    Paint damage tracking
```

---

### 14.1 Selector Matching Cache

Conceptually:

```text
(element, stylesheet_version)
        │
        ▼
Matched CSS Rules
```

Avoid repeatedly evaluating every selector when:

```text
nothing relevant changed
```

---

### 14.2 Computed Style Cache

Conceptually:

```text
(
    node,
    stylesheet_version,
    parent_style_version
)
        │
        ▼
ComputedStyle
```

---

### 14.3 Dirty Style Propagation

Example:

```text
Class changes
    │
    ▼
Target style dirty
```

Then determine whether descendants require invalidation.

If an inherited property changes:

```text
Target
 └── descendants may require recomputation
```

If a non-inherited property changes:

```text
Only target may need style recomputation
```

Avoid:

```text
Any style mutation
    ↓
Recompute entire document
```

when possible.

---

## 15. Typed CSS Properties

Avoid string-based CSS representations in layout hot paths.

Avoid:

```rust
HashMap<String, String>
```

during:

* layout;
* rendering;
* selector matching;
* style calculation.

Prefer:

```rust
enum PropertyId {
    Display,
    Width,
    Height,
    MarginTop,
    MarginRight,
    Color,
    Position,
    // ...
}
```

And typed values:

```rust
struct ComputedStyle {
    display: Display,
    width: Length,
    height: Length,
    position: Position,
    // ...
}
```

The pipeline should be:

```text
CSS text
    │
    ▼
Parser
    │
    ▼
Property IDs
    │
    ▼
Typed ComputedStyle
    │
    ▼
Layout
    │
    ▼
Paint
```

Not:

```text
Layout
    │
    ▼
HashMap<String, String>
```

---

## 16. String Interning and Atoms

Browser engines repeatedly process the same strings:

```text
div
span
input
class
id
style
href
display
position
```

Avoid repeated:

* heap allocations;
* string comparisons;
* hashing.

Introduce atoms or interned identifiers.

Example:

```rust
struct Atom(u32);
```

Possible internal usage:

```text
TagName
AttributeName
CSSProperty
ClassToken
```

The parser handles:

```text
String
```

and converts to:

```text
Atom / Enum / ID
```

The hot path should primarily use compact identifiers.

Benefits:

* faster comparisons;
* reduced allocations;
* reduced memory use;
* faster selector matching;
* faster DOM lookups.

---

## 17. Rendering Invalidation

The rendering pipeline should distinguish:

```text
Style dirty
Layout dirty
Paint dirty
Composite dirty
```

Example:

```text
Opacity animation
    ↓
Composite only

Color change
    ↓
Paint

Width change
    ↓
Layout + Paint

Display change
    ↓
Style + Layout + Paint
```

Do not treat all changes as equivalent.

Future architecture:

```text
Mutation
   │
   ▼
Style invalidation
   │
   ▼
Layout invalidation
   │
   ▼
Damage region generation
   │
   ▼
Paint only affected regions
   │
   ▼
Composite
```

---

## 18. Damage Tracking

Once paint invalidation exists, track affected regions.

Conceptually:

```rust
struct DamageRegion {
    rects: Vec<Rect>,
}
```

Or a more optimized representation later.

Example:

```text
Small button changes
    ↓
Invalidate button bounds
    ↓
Repaint affected region
```

Avoid:

```text
Small DOM update
    ↓
Repaint entire viewport
```

unless required.

---

## 19. JavaScript ↔ Rust Boundary Optimization

The JS/native boundary is likely to become a major performance hotspot.

Avoid repeated:

```text
JS value
    ↓
String conversion
    ↓
Rust String allocation
    ↓
DOM lookup
    ↓
Rust String allocation
    ↓
JS value
```

Optimize common paths.

Potential improvements:

* avoid temporary strings;
* use atoms;
* cache native handles;
* use typed conversions;
* avoid repeated prototype lookups;
* minimize FFI/native boundary crossings;
* batch operations where possible.

Benchmark separately:

```text
100k property reads
100k property writes
100k native function calls
100k event dispatches
```

---

## 20. Compatibility vs Performance

Maintain two separate development pipelines.

### Compatibility Pipeline

```text
Web API
   │
   ▼
Specification / behavior definition
   │
   ▼
Targeted WPT
   │
   ▼
Implementation
   │
   ▼
Regression tests
```

### Performance Pipeline

```text
Benchmark
   │
   ▼
Profile
   │
   ▼
Identify hotspot
   │
   ▼
Optimize
   │
   ▼
Measure before/after
   │
   ▼
Regression benchmark
```

Never accept an optimization solely because it:

> "looks faster"

Require measurable evidence.

---

## 21. Benchmark Suite

Maintain a dedicated benchmark suite.

---

### 21.1 DOM Benchmarks

```text
Create 1,000 nodes
Create 10,000 nodes
Append 10,000 nodes
Remove subtree
Move detached subtree
Clone subtree
querySelector
querySelectorAll
getElementsByClassName
innerHTML large tree
outerHTML replacement
```

---

### 21.2 JS ↔ Native Benchmarks

```text
100,000 DOM property reads
100,000 DOM property writes
100,000 native calls
100,000 event dispatches
classList mutations
dataset access
attribute access
```

---

### 21.3 CSS Benchmarks

```text
1,000 nodes / 100 rules
10,000 nodes / 500 rules
Deep selectors
Class mutations
Style mutations
Inherited property changes
```

---

### 21.4 Layout Benchmarks

```text
Static page
Single text mutation
Single class mutation
Viewport resize
Flex-heavy layout
Deep nested layout
Large list layout
```

---

### 21.5 Idle Game Benchmark

Because Nimble is optimized for lightweight and idle-game workloads, maintain a workload specifically representing the target use case.

Example:

```text
1,000 counters
Updates every 50ms
Frequent text mutations
Periodic class changes
Timers
Animation loop
Moderate DOM size
Long-running session
```

Measure:

```text
CPU usage
Frame time
Frame variance
Memory usage
Allocation rate
GC pressure
DOM update throughput
```

This benchmark should be considered more representative than generic browser benchmarks alone.

---

## 22. Profiling Requirements

Every significant optimization should be profiled.

Recommended workflow:

```text
Baseline benchmark
      │
      ▼
CPU profile
      │
      ▼
Memory profile
      │
      ▼
Flamegraph
      │
      ▼
Identify hot path
      │
      ▼
Optimize
      │
      ▼
Run same benchmark
      │
      ▼
Compare results
```

Record:

```text
Before
After
Percentage change
Regression risk
Affected workloads
```

Do not optimize based only on assumptions.

---

### 21.6 Benchmark Results (2026-09-12)

First real numbers for this section, produced against a stale-doc revisit request that questioned QuickJS and process-per-tab without evidence — see `PRODUCT.md`/CLAUDE.md, both already committed to these two choices. No swap followed; this only records the measurements that back keeping them.

**QuickJS hot-loop throughput** — `cargo bench -p js-runtime` (`crates/js-runtime/benches/hot_loop.rs`): a synthetic idle-game-shaped tick handler (3 accumulators, `sqrt`/`log`/`%` each tick, 200,000 ticks) ran in **~185ms** (~171–199ms range across samples) on the dev machine, release profile. That's on the order of 5M+ transcendental-shaped ops/sec from a plain interpreter (no JIT). Real idle games tick far less often (1–60 Hz) with much lighter math per tick — QuickJS has large headroom here. **Conclusion: no case for a JIT'd engine swap on JS throughput alone.**

**Per-process footprint** — `cargo run --release -p xtask -- bench-footprint` (`xtask/src/main.rs`): spawned 1 and then 5 real `profile-worker` processes against a fixed local test page, sampled via the existing `platform-apis::process_stats`:

```text
n=1: 114.6 MiB, ~0.4% CPU (idle)
n=5: 541.1 MiB total, ~108.2 MiB/profile avg, ~1.0% CPU total (idle)
```

Memory scales close to linearly per profile (no shared-baseline discount observed at n=5 vs n=1 in this quick run — expected, since each `profile-worker` is a fully separate process with its own JS runtime + GPU context). Idle CPU stays negligible even at 5 concurrent profiles. This is a minimal test page (no heavy JS/DOM), so it measures process/engine *baseline* overhead, not a worst case — but baseline overhead is exactly what "many isolated profiles running light" needs to be small. **Conclusion: process-per-tab's fixed per-process cost (~100+ MiB) is the number to watch as real target pages get tested; nothing here argues for switching to thread-per-tab yet, and doing so would cost a comparably large rewrite (see CLAUDE.md's Known gotchas) for an unproven gain.**

Both benchmarks are reusable — rerun `cargo bench -p js-runtime` and `cargo run --release -p xtask -- bench-footprint` whenever this question comes up again, instead of re-deciding from scratch.

---

[← back to spec/INDEX.md](../INDEX.md)
