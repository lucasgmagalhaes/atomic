# Mandatory Implementation Rules & Definition of Done

## 36. Mandatory Implementation Rules

### Architecture-First Rule

Before implementing a new Web API:

```text
1. Identify existing shared primitives.
2. Identify duplicated logic.
3. Extend shared infrastructure when possible.
4. Avoid API-specific state machines.
```

---

### Hot-Path Rule

Do not use:

```text
Heap-allocated strings
Repeated parsing
Repeated selector matching
Repeated HashMap lookups
```

in hot paths when a:

```text
Atom
Enum
Interned ID
Typed representation
Cache
```

can be used.

---

### Mutation Rule

All DOM mutations must use centralized DOM primitives.

Do not manually invalidate unrelated caches from individual JavaScript bindings.

---

### Cache Rule

Caches must use explicit:

```text
Versioning
Dirty state
Dependency information
```

Prefer:

```text
Mutation
    ↓
Mark dirty
    ↓
Lazy recomputation
```

over eager global synchronization.

---

### Node Lifetime Rule

Removing a node from the document must not automatically destroy its identity.

JavaScript references to detached nodes should remain valid.

---

### Rendering Rule

Do not trigger style, layout, and paint after every individual DOM mutation.

Batch mutations and render at scheduler-defined render opportunities.

---

### Resource Rule

Resource-consuming APIs should converge on:

```text
URL resolution
    ↓
Security
    ↓
Policy checks
    ↓
Cache
    ↓
Network
    ↓
Decode
```

---

### Performance Rule

Every performance optimization must have:

```text
Baseline
Benchmark
Profile
Before result
After result
Regression check
```

---

### Testing Rule

Every significant feature should include:

1. unit tests;
2. integration tests when crossing crates;
3. compatibility/WPT tests where applicable;
4. regression tests;
5. performance tests for hot paths.

---

## 37. Definition of Done

A feature should not be considered fully complete merely because:

```text
The API exists
```

A stronger definition is:

```text
API implemented
    │
    ├── Semantics defined
    ├── Unit tested
    ├── Integration tested
    ├── Compatibility tested
    ├── Lifetime behavior verified
    ├── Invalidations verified
    ├── Error behavior verified
    ├── Performance measured if hot
    └── Regression tests added
```

---

## 38. Final Engineering Principle

The goal of Atomic should not be:

> Implement as many browser APIs as possible.

The goal should be:

> Build a small number of powerful, reusable, well-profiled browser-engine primitives from which many Web APIs can be implemented cheaply and correctly.

The preferred development model is:

```text
Shared Primitive
      │
      ▼
Multiple APIs
      │
      ▼
Compatibility Tests
      │
      ▼
Benchmarks
      │
      ▼
Profiling
      │
      ▼
Optimization
      │
      ▼
Regression Protection
```

A good implementation should make the next implementation easier.

If a new feature requires:

* another global cache;
* another invalidation mechanism;
* another scheduler;
* another network pipeline;
* another object lifetime model;

then first investigate whether the existing architecture should be generalized instead.

---

## Final Priority Summary

```text
P0 — Architecture
    ├── Detached node lifetime
    ├── Generation-safe handles
    ├── Mutation pipeline
    ├── Dirty flags
    ├── Task scheduler
    ├── Resource loader
    └── Structured clone

P1 — Performance
    ├── Atoms
    ├── Selector cache
    ├── Computed style cache
    ├── Dirty propagation
    ├── Mutation batching
    ├── Damage tracking
    └── JS/native profiling

P2 — Compatibility
    ├── URL
    ├── Fetch core
    ├── AbortController
    ├── Script modes
    ├── Modules
    ├── Default actions
    └── Forms

P3 — Rendering
    ├── Scrolling
    ├── Viewport
    ├── Stacking contexts
    ├── z-index
    ├── Transforms
    ├── Incremental layout
    └── Compositing

P4 — Parallelism
    ├── MessagePort
    ├── MessageChannel
    ├── Transferables
    └── Workers

P5 — Large Platform Features
    ├── Grid
    ├── iframe
    ├── Multiple browsing contexts
    ├── Shadow DOM
    ├── Custom Elements
    └── MutationObserver
```

> **Core principle: every implementation should either improve a shared primitive or reuse one. New APIs should not continuously create new architecture.**

---

[← back to spec/INDEX.md](../INDEX.md)
