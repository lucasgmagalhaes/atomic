# Architecture Overview — Goals & the Architecture-Before-APIs Rule

## 1. Goals

Nimble already contains a significant amount of functionality across:

- JavaScript execution;
- DOM;
- events;
- CSS;
- layout;
- rendering;
- navigation;
- timers;
- networking;
- storage;
- Rust ↔ JavaScript bindings.

The primary risk going forward is no longer simply missing APIs.

The larger risk is architectural fragmentation:

```text
New API
   ↓
New binding-specific implementation
   ↓
New state
   ↓
New cache
   ↓
New invalidation logic
   ↓
More coupling
   ↓
Harder future implementations
````

Future work should prioritize:

1. Shared infrastructure over API-specific implementations.
2. Centralized mutation and invalidation.
3. Lazy recomputation where possible.
4. Typed and allocation-efficient hot paths.
5. Explicit ownership and object lifetime.
6. Measurable performance improvements.
7. Compatibility testing alongside implementation.
8. Infrastructure that can support multiple future Web APIs.

---

## 2. Architecture Before APIs

Before implementing a new Web API, first answer:

> Can this functionality reuse an existing shared internal primitive?

Do not prefer this architecture:

```text
API A
└── Custom state
    └── Custom scheduler
        └── Custom resource loader

API B
└── Custom state
    └── Custom scheduler
        └── Custom resource loader
```

Prefer:

```text
                    ┌─────────────────────┐
                    │ Shared Infrastructure│
                    ├─────────────────────┤
                    │ DOM Core            │
                    │ Mutation Pipeline   │
                    │ Task Scheduler      │
                    │ Resource Loader     │
                    │ Security Pipeline   │
                    │ Structured Clone    │
                    │ Style System        │
                    │ Layout System       │
                    └──────────┬──────────┘
                               │
             ┌─────────────────┼─────────────────┐
             │                 │                 │
             ▼                 ▼                 ▼
           fetch()            XHR             Workers
           Modules          Scripts        MessageChannel
```

### Mandatory rule

> Before creating API-specific infrastructure, inspect whether the behavior belongs to an existing or reusable subsystem.

Examples:

| New API          | Prefer Reusing                    |
| ---------------- | --------------------------------- |
| `fetch()`        | Resource Loader                   |
| XHR              | Fetch Core                        |
| ES Modules       | Resource Loader + Module Cache    |
| Workers          | Task Scheduler + Structured Clone |
| MessageChannel   | Structured Clone + Task Scheduler |
| History state    | Structured Clone                  |
| CSS updates      | Mutation Pipeline                 |
| Live collections | DOM versioning                    |
| Image loading    | Resource Loader                   |
| Script loading   | Resource Loader + Task Scheduler  |

---

---

[← back to spec/INDEX.md](../INDEX.md)
