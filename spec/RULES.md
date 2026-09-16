# Mandatory Rules (read before touching CEF binding / process / isolation code)

## Before writing code

1. **Don't reimplement what CEF already gives you.** Cookie jars, localStorage/IndexedDB, HTTP cache, DNS/proxy resolution, TLS, HTML/CSS/JS execution — all of it is CEF's job now. If you find yourself writing a parser, cache, or storage engine for something the web platform already needs, stop: it belongs in `crates/cef`'s binding surface (calling the real CEF API), not hand-rolled in Rust. This is the direct opposite of the old `neutron` rule set and the whole point of the pivot.
2. **Isolation is the product — verify it, don't assume it.** Every new per-profile feature (storage, network, permissions, cache) must go through a per-profile `CefRequestContext`, never a shared/default one. If a CEF API doesn't obviously expose per-context scoping, check upstream CEF docs/source before assuming it's global — a silently-shared cache or cookie jar is a correctness bug for this product, not a minor gap.
3. **Startup cost is a first-class metric.** Any new subsystem initialized eagerly at app or profile start needs a reason it can't be lazy/deferred. Check `spec/ROADMAP.md` P2's startup budget before adding anything to the hot startup path.
4. **Idle cost is a first-class metric.** A background/hidden profile should cost near-zero CPU and minimal memory. Anything that polls, ticks, or keeps a background profile's renderer fully warm needs a justification against `spec/ROADMAP.md` P3's throttling design, not a default "just keep it running."
5. **No permanent fake capabilities.** Don't stub a CEF feature (permissions prompt, download handling, devtools protocol) with a silent no-op that looks like it works. Either wire the real CEF callback or leave it visibly unimplemented and tracked in `spec/ROADMAP.md`.
6. **Sandbox/process boundaries are security boundaries, not just performance ones.** Don't collapse CEF's multi-process model (e.g. `--single-process`) for convenience — that flag exists and is tempting for debugging, but it defeats both the sandbox and the per-profile isolation this product sells. Never ship it enabled.

## Code quality (SOLID / DRY / KISS / YAGNI)

- **SRP** — `crates/cef` owns the raw CEF binding surface (thin wrappers over the C API, no product logic). `crates/profile`/`profile-worker` (or their CEF-era successor) own the per-profile process/lifecycle logic. `crates/atomic` owns the GUI shell and composes panes from whatever `crates/cef` gives it. Don't blur these — a GUI-layer file calling raw CEF C functions directly is a layering violation.
- **DIP** — `crates/atomic` should depend on a focused profile/browser-context API (open, navigate, close, get-frame-buffer), not on CEF types directly sprinkled through GUI code. Keep the CEF C API contained to `crates/cef` and a thin adapter layer.
- **KISS** — prefer CEF's default behavior over a custom reimplementation unless there's a measured, documented reason (a specific isolation gap, a specific startup-cost problem) — see rule 1.
- **YAGNI** — don't build abstraction for multiple browser engines (CEF vs. something else) unless a second engine is actually on the roadmap. This project committed to CEF; don't pay an abstraction tax for a hypothetical engine swap that isn't planned.

## Definition of done

```text
Feature implemented against the real CEF API (not stubbed)
    ├── Per-profile isolation verified (if it touches storage/network/permissions)
    ├── Startup-cost impact measured (if it runs at app/profile init)
    ├── Idle-cost impact measured (if it runs continuously or per-frame)
    ├── Unit/integration tested (crate/tests/<file>_test.rs convention, per CLAUDE.md)
    ├── Error behavior verified, not just the happy path
    └── spec/ROADMAP.md item flipped from `[ ]` to `[x]`/`[~]` with a one-line note
```

## Validate before committing

`cargo build --workspace` and `cargo test --workspace`. `cargo fmt --all` — the pre-commit hook enforces it. CEF binary distribution must be present/fetched locally for anything that links against it — document the fetch step in `crates/cef/README.md` so a fresh clone can reproduce the build.

---

[← back to spec/INDEX.md](INDEX.md)
