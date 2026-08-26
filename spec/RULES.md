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
