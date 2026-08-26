# Mandatory Rules (read before implementing anything from ROADMAP.md)

Full detail: [architecture/rules-and-dod.md](architecture/rules-and-dod.md). Native-binding-specific footguns (quickjs class IDs, prototype lifetime, refcount leaks, re-entrant `JS_Eval`): see `CLAUDE.md`'s **Known gotchas** section, not repeated here.

## Before writing code

1. **Architecture-first.** Before adding a new Web API: identify an existing shared primitive it can reuse (see [architecture/primitives.md](architecture/primitives.md)'s reuse table). Do not build a new API-specific state machine/cache/scheduler if an existing one fits.
2. **Hot-path rule.** No heap-allocated strings / repeated parsing / repeated selector matching / repeated `HashMap` lookups in layout, style, or paint hot paths — use an atom, enum, interned ID, or cache instead ([performance.md](architecture/performance.md)).
3. **Mutation rule.** All DOM mutations go through centralized `dom::Dom` primitives. Never manually invalidate an unrelated cache from inside a JS binding function.
4. **Cache rule.** Any new cache needs explicit versioning/dirty-state, invalidated lazily on next access — not eager global rebuilds on every mutation.
5. **Node lifetime rule.** Removing a node from the document must not destroy its identity (this is currently **violated** — see ROADMAP P0 item 1 — do not add more code that assumes destructive removal).
6. **Rendering rule.** Don't trigger style/layout/paint after every individual mutation; batch and render at defined render opportunities (the ~60Hz `profile-worker` loop already gives this for free in most cases — don't add a new relayout-on-every-write path).
7. **Resource rule.** Anything that fetches a resource should go through the same URL-resolution → security/CSP/mixed-content → cache → network → decode shape `net`/`profile-worker` already use, not a new one-off fetch path.
8. **No permanent fake APIs.** A no-op stub that silently pretends to work is worse than an error. If a compatibility stub is genuinely necessary: document it as a stub, test it, and track it as incomplete in the matrix — don't claim it's implemented.
9. **Don't accept a binding limitation without checking upstream first.** Before declaring something "impossible" because `quickjs-sys` doesn't expose it: check whether QuickJS-ng's C API already has what's needed and whether a minimal binding can be added (see `crates/js-runtime/quickjs-sys/src/lib.rs` for the existing pattern — most bindings here are hand-added, not exhaustive).

## Code quality (SOLID / DRY / KISS / YAGNI)

Concrete translations of these into how this specific codebase is already built — not generic textbook restatements. Follow the existing pattern, don't reinvent it per-file.

- **SRP** — one module per concern, already the convention: `dom`/`css`/`layout-engine`/`render` are separate crates; `js-runtime/src/dom_bindings/` is itself split into `node_registry.rs`/`element_classes.rs`/`attributes.rs`/`content.rs`/`forms.rs`/etc rather than one god-file. A new binding area gets its own file with its own `register()`, not a new branch bolted onto an existing one.
- **OCP** — extend via a new registration function, not by editing an existing dispatch switch. `class_registry::ensure_class`/`class_id_for` is the extension point every new QuickJS class goes through; adding a class means writing a new `ensure_*_class` following the existing idempotency pattern (see `CLAUDE.md`'s Known Gotchas — prototype-per-Context), not modifying the registry itself.
- **LSP** — a subclass must behave substitutably for its parent everywhere the parent is accepted. This is the exact property the `Node`→`Element`→`HTMLElement`→`HTMLInputElement` prototype-chain bug (fixed this session, see git log) violated: `instanceof HTMLInputElement` held but `instanceof Node` didn't, because the "everywhere a supertype is checked, a subtype instance must pass" contract broke on the second registration. Any new interface-hierarchy class must keep this holding — test the full `instanceof` chain, not just the leaf.
- **ISP** — keep a binding module's public surface to what its own class actually needs. Don't add a shared "kitchen sink" trait/struct that every binding module depends on just to get one method; the `define_method`/`define_getter`/`define_class_list`-style narrow helpers already in `request_response.rs`/`dom_bindings/` are the right shape.
- **DIP** — depend on a focused subsystem API, not a growing generic bag of state. This is exactly [architecture/primitives.md §13 "Host State Modularization"](architecture/primitives.md) — `HostState` must not become "add one more field for every feature"; a new cross-cutting concern gets its own `XState` struct with its own focused methods.
- **DRY** — see the *Architecture-first* rule above: reuse an existing shared primitive before writing a new one. At the smaller scale, if you're about to write a third near-identical `JS_NewCFunction2`/`JS_DefinePropertyGetSet` registration by hand, factor a helper first (the codebase already has several: `define_method`, `define_getter`, `expose_constructor`) — don't add a fourth copy-pasted variant.
- **KISS** — the plainest implementation that passes its tests and matches the matrix's documented scope beats a generic abstraction nobody asked for. A documented deviation (e.g. "no `Proxy` support, so `dataset` writes don't reflect back") is an acceptable, honest simplification; a silent no-op that pretends to be the real thing is not (see "No permanent fake APIs" above).
- **YAGNI** — don't build speculative generality ahead of a second real caller. Structured Clone (P0 item 7) is the one deliberate exception on the roadmap — it's called out *because* three different features (`postMessage`, `history.pushState`, Workers) already need the same mechanism, which is the actual DRY-justified case for building shared infrastructure early, not a default license to generalize everything up front.

## Definition of done

A feature is not done just because the API exists. It needs, in order:

```text
API implemented
    ├── Semantics defined (what does it actually do, not just its shape)
    ├── Unit tested (crate/tests/<file>_test.rs, not inline #[cfg(test)])
    ├── Integration tested (if it crosses crates)
    ├── Lifetime behavior verified (does a stale reference misbehave?)
    ├── Invalidation verified (does a mutation correctly dirty what depends on it?)
    ├── Error behavior verified (not just the happy path)
    ├── Performance measured, if it's a hot path
    └── Matrix updated: flip the `- [ ]` to `- [x]` in the relevant spec/matrix/*.md file
```

Also update `CLAUDE.md`'s "Implementation status" if the item is one of the non-JS-engine crates it tracks (ipc/profile/net/security/platform-apis/apps-shell/workers/storage) — see `.opencode/worker-prompt.md` for the exact doc-update step an automated worker follows after landing a task.

## Validate before committing

`cargo build --workspace` and `cargo test --workspace` (add `--exclude automation` only if that crate is mid-edit by someone else). `cargo fmt --all` — the pre-commit hook enforces it.

---

[← back to spec/INDEX.md](INDEX.md)
