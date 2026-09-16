# Neutron encapsulation — architectural review plan

Status: proposal, not yet implemented.

## Problem

Repo has three real scopes but only one (`atomicjs`) is actually encapsulated:

1. **Browser shell** (`apps/shell`) — the GUI app.
2. **atomicjs** — experimental JS compiler/VM. Already isolated: `crates/atomicjs/Cargo.toml` declares its own empty-members `[workspace]`, decoupled from the root workspace on purpose (see `spec/proposals/ATOMIC_JS_SPIKE.md` §6). Just missing a README.
3. **Compile/render/execute infra** (css, html, dom, atoms, layout-engine, render, webgl, image_decode, js-runtime, quickjs-sys, workers) — currently 11 separate root-workspace members with no facade. `apps/shell` path-deps directly on `dom`, `html`, `css`, `layout-engine`, `render`, `quickjs-sys` instead of going through `js-runtime`. No single crate owns "turn HTML/CSS/JS source into rendered frames" — it's scattered flat across `crates/`.

## Target shape

New crate **`crates/neutron`** becomes the facade/boundary for scope 3. It path-deps on the 11 crates above and re-exports a curated public API:

```
neutron::html    -> html5ever TreeSink / parsing
neutron::css     -> parser, cascade, at-rules
neutron::dom     -> dom::Dom, atoms
neutron::layout  -> layout-engine
neutron::paint   -> render
neutron::gl      -> webgl
neutron::image   -> image_decode
neutron::js      -> js-runtime (Runtime/Context, quickjs-sys bridge)
neutron::workers -> workers (OS-thread Web Workers)
```

Everything outside `crates/neutron/` (shell, profile, profile-worker, automation) depends on `neutron` only — never on `dom`/`css`/`html`/`layout-engine`/`render`/`webgl`/`image_decode`/`js-runtime`/`workers`/`quickjs-sys` directly.

Crates left alone (already correctly scoped, not part of this cleanup): `net`, `storage`, `platform-apis`, `security`, `ipc`, `import`, `profile`, `automation`. These are platform/orchestration services `neutron` consumes as external deps (`js-runtime` already path-deps on `net`/`storage`/`platform-apis` for Web API backing — that's correct and stays).

## Physical move — Option A (revised decision)

Original draft of this plan proposed a facade-only approach (crates stay flat under `crates/*`, `neutron` just re-exports). Revisited: repo already has the nested-member pattern — `crates/js-runtime/quickjs-sys` is a root-workspace member nested inside `crates/js-runtime/`. So physically nesting the 11 crates under `crates/neutron/*/` is not a new convention, it matches existing precedent, and it makes the three scopes line up as three top-level concepts: `apps/shell`, `crates/atomicjs`, `crates/neutron/*`. Going with **physical move**.

Cost is lower than it first looks:
- `git mv` preserves rename tracking (`git log --follow`, `git blame --follow` both still work).
- No Rust source hardcodes cross-crate paths outside `Cargo.toml`'s `path = "../..."` deps — no `include!`/`include_str!` crossing these crate boundaries.
- One extra step: the `quickjs-ng` git submodule lives at `crates/js-runtime/quickjs-sys/vendor/quickjs-ng` (`.gitmodules`). Moving `js-runtime` to `crates/neutron/js-runtime` means updating that `.gitmodules` path and running `git submodule sync` — do this in the same commit as the `js-runtime` move so the submodule is never left dangling.

Target layout:

```
crates/neutron/Cargo.toml          <- facade crate, package "neutron"
crates/neutron/src/lib.rs          <- re-exports per module (html/css/dom/layout/paint/gl/image/js/workers)
crates/neutron/css/
crates/neutron/html/
crates/neutron/dom/
crates/neutron/atoms/
crates/neutron/layout-engine/
crates/neutron/render/
crates/neutron/webgl/
crates/neutron/image_decode/
crates/neutron/js-runtime/
crates/neutron/js-runtime/quickjs-sys/   <- unchanged relative nesting, submodule path updated
crates/neutron/workers/
```

Root `Cargo.toml` `[workspace] members` updates from `"crates/css"`, `"crates/dom"`, etc. to `"crates/neutron/css"`, `"crates/neutron/dom"`, etc. Internal path deps between the 11 (e.g. `layout-engine` -> `css`, `render` -> `layout-engine`) are unaffected since they move together and stay siblings (`../css` still resolves).

## Steps

1. `git mv` each of the 11 crate directories into `crates/neutron/<name>/`. Update root `Cargo.toml` members list to the new paths. Update `.gitmodules` submodule path for `quickjs-ng` + `git submodule sync`. One commit — purely mechanical, no logic change. Verify `cargo build --workspace` still green (internal sibling path deps unaffected).
2. `crates/neutron/Cargo.toml` (the facade, at `crates/neutron/`, sibling to the 11 moved dirs — not nested inside any of them) + `src/lib.rs`: path-dep on all 11, re-export curated API per module (`neutron::html`, `neutron::css`, `neutron::dom`, `neutron::layout`, `neutron::paint`, `neutron::gl`, `neutron::image`, `neutron::js`, `neutron::workers`). One commit.
3. `apps/shell`: replace direct path deps (`dom`, `html`, `css`, `layout-engine`, `render`, `js-runtime`, `quickjs-sys`, all now at `crates/neutron/*`) with a single `neutron = { path = "../../crates/neutron" }`; update `use` paths (`chrome_engine.rs` etc.) to `neutron::*`.
4. `crates/profile` / `profile-worker`: same swap — depend on `neutron` instead of `js-runtime` directly.
5. `crates/automation`: audit for direct deps on the 11, swap to `neutron` if any.
6. Boundary check: small script in `xtask` that greps every `Cargo.toml` outside `crates/neutron/` for `path = ".../neutron/{css,html,dom,atoms,layout-engine,render,webgl,image_decode,js-runtime,workers}"` (anything reaching past the facade into a sub-crate) and fails. Wire into `.cargo-husky/hooks/pre-commit` alongside the existing `cargo fmt`/`dprint` checks.
7. READMEs (new, none currently exist for any crate):
   - `apps/shell/README.md` — GUI app: panes/workspaces/downloads/history/i18n/settings; renders via `neutron`, uses `net`/`storage`/`security`/`platform-apis`/`import`/`automation`/`profile` for platform services.
   - `crates/atomicjs/README.md` — experimental JS compiler/VM (lexer/parser/bytecode/tiered execution), deliberately isolated single-crate workspace, not built by `cargo build --workspace`, how to build/bench standalone (`cargo bench` inside `crates/atomicjs`), relation to `neutron::js`'s quickjs-ng (production engine vs research spike).
   - `crates/neutron/README.md` — pipeline diagram (HTML/CSS text -> DOM -> layout -> paint/webgl, JS execution via quickjs-ng + Workers), lists the 11 sub-crates and their role, points to `spec/matrix/*.md` for capability detail.
8. Update root `CLAUDE.md` "Source of truth for architecture" section: note the three scopes (`apps/shell`, `crates/atomicjs`, `crates/neutron/*`) and that `neutron` is the dependency boundary for compile/render/execute crates.
9. `cargo build --workspace && cargo test --workspace` after each step; `graphify update .` after landing.

## Sequencing

One commit per step (matches repo convention: one crate/domain per commit). Step 1 (the move) first and alone, verified green, before touching any dependency logic — keeps the mechanical rename separable from the facade/boundary work in history. Steps 3–5 are the risky ones (touch call sites) — do shell first, verify green, then profile, then automation.
