# Recommended Implementation Priority (P0-P5, Detailed)

## 30. Recommended Implementation Priority

The following order prioritizes architecture, performance, and future reuse.

---

### P0 — Architectural Foundations

#### 1. Non-destructive detached DOM nodes

Implement:

```text
Connected
Detached
Destroyed
```

without destroying nodes on ordinary removal.

#### 2. Generation-safe Node handles

Use:

```text
(NodeId, Generation)
```

or equivalent.

#### 3. Central Mutation Pipeline

All DOM changes must use centralized invalidation.

#### 4. Dirty Flags

Classify:

```text
DOM
Collections
Selectors
Style
Layout
Paint
Accessibility
```

#### 5. Task Scheduler

Central event loop and task queues.

#### 6. Resource Loader

Shared loading pipeline.

#### 7. Structured Clone

Before Workers and MessageChannel.

---

## 31. P1 — Performance Infrastructure

#### 8. String Interning / Atoms

Intern:

```text
Tags
Attributes
CSS properties
Common identifiers
```

#### 9. Selector Cache

Cache selector matching.

#### 10. Computed Style Cache

Cache typed computed styles.

#### 11. Dirty Style Propagation

Avoid full document style recomputation.

#### 12. DOM Mutation Batching

Avoid layout after every mutation.

#### 13. Paint Damage Tracking

Track affected regions.

#### 14. JS ↔ Rust Boundary Benchmarks

Measure native binding costs.

---

## 32. P2 — High-Impact Compatibility

#### 15. URL

Implement:

* `URL`;
* `URLSearchParams`;
* relative resolution;
* base URL behavior.

#### 16. Fetch Core

Implement:

* Request;
* Response;
* Headers;
* Body;
* cancellation.

#### 17. AbortController

Shared cancellation primitive.

#### 18. Script Loading Modes

Implement:

* parser blocking;
* defer;
* async.

#### 19. ES Modules

Implement:

* resolver;
* import maps;
* dynamic import;
* module cache.

#### 20. Default Actions

Implement correct browser default behavior.

#### 21. Form Semantics

Expand real form behavior.

---

## 33. P3 — Rendering

#### 22. Real Scrolling

Implement:

* scroll position;
* scroll APIs;
* scrollIntoView;
* scroll offsets.

#### 23. Viewport

Implement real viewport state.

#### 24. Stacking Contexts

Implement stacking context behavior.

#### 25. z-index

Implement correct ordering.

#### 26. Transforms

Implement transform pipeline.

#### 27. Incremental Layout

After style invalidation infrastructure is stable.

#### 28. Compositing Layers

Where performance benefits justify it.

---

## 34. P4 — Parallelism

#### 29. MessagePort

#### 30. MessageChannel

#### 31. Structured Clone Integration

#### 32. Transferable Objects

#### 33. Worker

Implement only after the previous primitives exist.

---

## 35. P5 — Large Platform Features

#### 34. CSS Grid

#### 35. iframe

#### 36. Multiple Browsing Contexts

#### 37. Cross-window Messaging

#### 38. Shadow DOM

#### 39. Custom Elements

#### 40. MutationObserver

These features should be implemented on top of stable primitives rather than introducing parallel architecture.

---

---

[← back to spec/INDEX.md](../INDEX.md)
