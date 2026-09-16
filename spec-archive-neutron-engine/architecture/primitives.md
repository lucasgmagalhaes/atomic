# Core Architectural Primitives (Mutation Pipeline, Node Lifetime, Scheduler, Resource Loader, Fetch, Structured Clone, Host State)

## 3. Core Architectural Primitives

The following primitives should be considered foundational infrastructure.

---

### 3.1 DOM Mutation Pipeline

All DOM mutations should pass through centralized DOM mutation primitives.

The intended flow is:

```text
JavaScript API
      │
      ▼
DOM Mutation
      │
      ▼
Mutation Classification
      │
      ├── Collection invalidation
      ├── Selector invalidation
      ├── Style invalidation
      ├── Layout invalidation
      ├── Paint invalidation
      └── Accessibility invalidation
```

Avoid scattered code like:

```rust
element.set_attribute(...);

invalidate_layout();
invalidate_style();
clear_collection_cache();
clear_selector_cache();
```

inside multiple JavaScript bindings.

Prefer:

```rust
dom.set_attribute(node, name, value);
```

with centralized mutation processing.

---

### 3.2 Mutation Classification

Not every mutation requires a complete render pipeline invalidation.

Introduce explicit mutation metadata or derived dirty flags.

Example:

```rust
bitflags! {
    struct DirtyFlags: u32 {
        const DOM        = 1 << 0;
        const COLLECTION = 1 << 1;
        const SELECTORS  = 1 << 2;
        const STYLE      = 1 << 3;
        const LAYOUT     = 1 << 4;
        const PAINT      = 1 << 5;
        const A11Y       = 1 << 6;
    }
}
```

A mutation can then derive:

```text
setAttribute("class")
    ↓
DOM
COLLECTION
SELECTORS
STYLE
LAYOUT
PAINT
```

While:

```text
scrollTop change
    ↓
PAINT
```

And:

```text
text node update
    ↓
DOM
LAYOUT
PAINT
```

The system should avoid invalidating unrelated subsystems.

---

### 3.3 Lazy Invalidation

Prefer:

```text
Mutation
   ↓
Increment version / mark dirty
   ↓
Continue execution
   ↓
Recompute only when required
```

Over:

```text
Mutation
   ↓
Immediately rebuild
   ├── Collections
   ├── Selectors
   ├── Styles
   ├── Layout
   └── Paint
```

Example:

```rust
struct VersionedCache<T> {
    version: u64,
    value: T,
}
```

A cache should be recomputed when:

```rust
if cache.version != current_version {
    cache.recompute();
}
```

---

## 4. DOM Node Lifetime and Identity

### 4.1 Non-destructive Node Removal

Removing a node from the DOM should not immediately destroy the node.

A node can exist in different states:

```text
Alive + Connected
Alive + Detached
Destroyed
```

For example:

```rust
enum NodeState {
    Connected,
    Detached,
    Destroyed,
}
```

Expected behavior:

```javascript
const node = parent.removeChild(child);

// node must remain usable

otherParent.appendChild(node);
```

Desired implementation:

```text
removeChild()
     │
     ▼
Remove parent relationship
     │
     ▼
Mark node as detached
     │
     ▼
Preserve Node identity
     │
     ▼
Existing JavaScript references remain valid
```

Do not destroy a node merely because it is removed from the document tree.

---

### 4.2 Why Detached Nodes Matter

Correct detached-node behavior is foundational for:

* `removeChild`;
* `appendChild`;
* `insertBefore`;
* `replaceChild`;
* `replaceWith`;
* `DocumentFragment`;
* `cloneNode`;
* `adoptNode`;
* future Shadow DOM;
* future Range APIs;
* Selection APIs;
* drag-and-drop;
* template content;
* MutationObserver.

Implementing these features around destructive removal creates increasing complexity and future rewrites.

---

## 5. Node Identity Safety

If the DOM uses arena allocation and numeric `NodeId`s, avoid stale JavaScript wrappers becoming valid for a newly allocated node.

Instead of:

```text
NodeId = 42
```

prefer a generation-aware identity:

```rust
struct NodeHandle {
    id: NodeId,
    generation: u32,
}
```

Example:

```text
Node #42, generation 1
        │
        ▼
Node removed
        │
        ▼
Arena slot reused
        │
        ▼
Node #42, generation 2
```

An old JavaScript wrapper containing:

```text
(42, generation 1)
```

must not access:

```text
(42, generation 2)
```

This prevents stale object identity bugs.

---

## 6. JavaScript Native Object Lifetime

Avoid relying exclusively on manually clearing multiple caches such as:

```text
NODE_OBJECTS
CLASS_LIST_OBJECTS
DATASET_OBJECTS
ATTRIBUTES_OBJECTS
EVENT_LISTENERS
```

Instead, define a consistent native object lifecycle model.

Recommended direction:

```text
JS Object
    │
    ▼
Native Handle
    │
    ├── NodeHandle
    ├── DocumentHandle
    ├── CollectionHandle
    └── Host Object Handle
```

Native handles should validate:

1. object type;
2. context ownership;
3. node generation;
4. lifetime validity.

This reduces the amount of manual cache invalidation required by DOM operations.

---

## 7. Live DOM Collections

Do not implement live collections by eagerly rebuilding every collection after every DOM mutation.

Avoid:

```text
DOM mutation
    │
    ├── Update HTMLCollection A
    ├── Update HTMLCollection B
    ├── Update NodeList C
    └── Update NodeList D
```

Prefer lazy versioned collections.

Example:

```rust
struct LiveCollection {
    root: NodeId,
    query: CollectionQuery,
    last_dom_version: u64,
    cached_nodes: Vec<NodeHandle>,
}
```

On access:

```rust
if collection.last_dom_version != dom.version() {
    collection.recompute(dom);
}
```

This provides:

* live behavior;
* stable collection identity;
* lazy recomputation;
* reduced mutation cost.

---

### 7.1 Future Optimization: Subtree Versions

A global DOM version is a useful first step.

Later, optimize with subtree versions:

```text
Document
 ├── Header subtree version: 10
 ├── Main subtree version: 42
 └── Footer subtree version: 7
```

A collection rooted in:

```text
Main
```

does not need invalidation when:

```text
Footer
```

changes.

---

## 8. DOM Mutation Batching

A JavaScript loop such as:

```javascript
for (let i = 0; i < 10000; i++) {
    parent.appendChild(createElement("div"));
}
```

must not cause:

```text
10,000 style calculations
10,000 layouts
10,000 paints
```

Instead:

```text
JavaScript execution
        │
        ▼
DOM mutations accumulate
        │
        ▼
Microtask checkpoint
        │
        ▼
Style update
        │
        ▼
Layout
        │
        ▼
Paint
```

Conceptually:

```rust
begin_mutation_batch();

run_script();

end_mutation_batch();
```

This batching should ideally be automatic from page JavaScript's perspective.

---

## 9. Task Scheduler

Before implementing complex asynchronous APIs, establish a central task scheduler.

Suggested queues:

```text
Task Scheduler
├── Macrotasks
├── Microtasks
├── Timers
├── Network completions
├── Animation frames
├── Rendering tasks
└── Lifecycle tasks
```

Possible representation:

```rust
enum Task {
    Script(...),
    Timer(...),
    Network(...),
    AnimationFrame(...),
    Lifecycle(...),
}
```

Suggested execution flow:

```text
Select task
    │
    ▼
Execute task
    │
    ▼
Run JavaScript
    │
    ▼
Drain microtasks
    │
    ▼
Apply pending DOM work
    │
    ▼
Determine render opportunity
    │
    ▼
Run requestAnimationFrame
    │
    ▼
Style / Layout / Paint
```

This should become the basis for:

* Promises;
* timers;
* fetch;
* modules;
* async scripts;
* Workers;
* MessageChannel;
* lifecycle events.

---

## 10. Resource Loader

All resource loading should converge on a shared pipeline.

```text
Resource Request
       │
       ▼
URL Resolution
       │
       ▼
Security Policy
       │
       ▼
CSP
       │
       ▼
Mixed Content
       │
       ▼
Referrer Policy
       │
       ▼
Cache
       │
       ▼
Network
       │
       ▼
Decode
```

Suggested abstraction:

```rust
struct ResourceRequest {
    url: Url,
    initiator: Initiator,
    destination: Destination,
    priority: Priority,
    cors_mode: CorsMode,
    referrer_policy: ReferrerPolicy,
}
```

This pipeline should be reusable by:

* HTML;
* CSS;
* JavaScript scripts;
* ES modules;
* images;
* `fetch()`;
* XHR;
* future media APIs.

---

## 11. Fetch Architecture

Do not independently implement networking logic for:

* `fetch`;
* XHR;
* modules;
* future APIs.

Create a shared Fetch Core:

```text
Fetch Core
├── Request
├── Response
├── Headers
├── Body
└── Stream
```

Then expose adapters:

```text
Fetch Core
   │
   ├── fetch()
   ├── XMLHttpRequest
   ├── Module Loader
   └── Future APIs
```

This prevents duplicated implementations of:

* URL handling;
* headers;
* redirects;
* cancellation;
* body consumption;
* CORS;
* errors.

---

## 12. Structured Clone Before Workers

Do not implement Workers first.

Implement:

```text
1. Structured Clone
2. Transferable abstraction
3. MessagePort
4. MessageChannel
5. Worker
```

Structured Clone should be reusable for:

* `postMessage`;
* `MessageChannel`;
* Workers;
* `history.pushState`;
* `history.replaceState`;
* storage events;
* future cross-context messaging.

Avoid storing history state merely as a direct JavaScript object reference when the engine can instead use the shared clone mechanism.

---

## 13. Host State Modularization

Avoid allowing `HostState` to become a large container for unrelated features.

Do not converge toward:

```rust
struct HostState {
    // URL
    // history
    // CSP
    // permissions
    // layout
    // styles
    // navigation
    // trusted types
    // storage
    // ...
}
```

Prefer subsystem composition:

```rust
struct PageState {
    navigation: NavigationState,
    security: SecurityState,
    lifecycle: LifecycleState,
    layout: LayoutState,
    storage: StorageState,
}
```

Each subsystem should expose focused APIs:

```rust
page.navigation.navigate(...);
page.security.check_request(...);
page.lifecycle.transition(...);
page.layout.get_rect(...);
```

Benefits:

* reduced coupling;
* clearer ownership;
* easier testing;
* easier profiling;
* simpler future replacement.

---

---

[← back to spec/INDEX.md](../INDEX.md)
