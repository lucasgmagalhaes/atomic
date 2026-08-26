# Compatibility Infrastructure (Script Loading, ES Modules, Security Pipeline, Stubs Policy, WPT)

## 23. Script Loading Architecture

Do not implement:

* parser-blocking scripts;
* `defer`;
* `async`;
* modules;

as completely separate systems.

Use:

```text
HTML Parser
     │
     ▼
Script Discovery
     │
     ▼
Resource Loader
     │
     ▼
Script Scheduler
     │
     ├── Blocking
     ├── Async
     ├── Defer
     └── Module
```

The scheduler should determine execution order.

---

## 24. ES Module Architecture

Implement a reusable module system:

```text
Module URL
    │
    ▼
Resolver
    │
    ▼
Import Map
    │
    ▼
Resource Loader
    │
    ▼
Module Cache
    │
    ▼
Instantiation
    │
    ▼
Evaluation
```

The cache should be keyed by resolved module identity.

Avoid:

```text
Every import
    ↓
Fetch again
    ↓
Parse again
    ↓
Evaluate again
```

---

## 25. Security Pipeline

Security should not be implemented separately for every API.

Centralize checks.

```text
Operation
    │
    ▼
Origin Check
    │
    ▼
CORS
    │
    ▼
CSP
    │
    ▼
Permissions Policy
    │
    ▼
Secure Context
    │
    ▼
Operation
```

Examples:

```text
fetch()
Image loading
Module loading
Script loading
Clipboard
Notifications
Future APIs
```

should reuse centralized policy evaluation.

---

## 26. Avoid Permanent Fake APIs

Do not add silent no-op APIs merely to avoid exceptions.

Example:

```javascript
element.scrollIntoView();
```

A no-op implementation can cause application bugs that are difficult to diagnose.

If a compatibility stub is necessary:

1. explicitly classify it as a stub;
2. document it;
3. test it;
4. track it as incomplete;
5. avoid claiming the feature is implemented.

Feature states should be:

```text
Implemented
Partial
Compatibility Stub
Unsupported
```

Prefer semantically correct partial implementations over APIs that silently pretend to work.

---

## 27. Dataset and Exotic Property Behavior

Do not permanently accept API limitations solely because a current Rust binding does not expose a convenient mechanism.

Before declaring an API impossible or heavily simplified:

1. inspect the underlying QuickJS API;
2. determine whether the missing functionality exists upstream;
3. check whether a minimal binding can be added;
4. investigate property hooks or exotic objects;
5. only then accept a documented limitation.

This is especially relevant for:

* `dataset`;
* bracket access;
* live property reflection;
* DOM exotic objects;
* named properties;
* collection behavior.

A small missing native binding may unlock multiple Web APIs.

---

## 28. Web Platform Tests

WPT should begin before the engine reaches "platform readiness."

Start immediately with targeted subsets.

Recommended areas:

```text
DOM
Events
Selectors
URL
History
Timers
Fetch
Forms
Collections
```

Workflow:

```text
Implement feature
      │
      ▼
Run targeted WPT subset
      │
      ▼
Fix compatibility failures
      │
      ▼
Add Nimble regression tests
      │
      ▼
Track compatibility percentage
```

Do not wait until late development to discover that multiple APIs have incompatible semantics.

---

## 29. Automatic Capability Tracking

Where practical, capability reporting should derive from:

```text
Tests
+
Implementation status
```

rather than manual percentage estimates only.

Example:

```text
Feature:
    implemented: true
    unit_tests: true
    integration_tests: true
    WPT_pass_rate: 82%
    performance_benchmark: true
```

This makes roadmap progress measurable.

---

---

[← back to spec/INDEX.md](../INDEX.md)
