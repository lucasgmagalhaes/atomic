# neutron

The browser's compile/layout/paint/execute pipeline: turns HTML/CSS/JS
source into rendered frames, and runs JS against the DOM it builds.

```
html source ─▶ html ──▶ dom ──▶ layout-engine ──▶ render ──▶ pixels
css source  ─▶ css  ───────────────┘                 │
                                                       └─▶ webgl (canvas/WebGL paths)
js source   ─▶ js-runtime (quickjs-ng) ──▶ mutates dom, reads layout/render
                    │
                    └─▶ workers (OS-thread Web Workers, each with their own js-runtime)

image_decode: shared PNG/JPEG decode, consumed by layout-engine (intrinsic sizing) and render (painting)
atoms: interned string handles for hot-path tag/attribute names, consumed by dom/css
```

## Boundary

This crate is the dependency boundary for the eleven sub-crates above.
Everything outside `crates/neutron/` (`crates/atomic`, `crates/profile`,
`crates/automation`) depends on `neutron` only — never on `css`, `html`,
`dom`, `atoms`, `layout-engine`, `render`, `webgl`, `image_decode`,
`js-runtime`, `js-runtime/quickjs-sys`, or `workers` directly. Enforced by
`cargo run -p xtask -- check-neutron-boundary` (wired into the pre-commit
hook).

`neutron`'s own `src/lib.rs` re-exports each sub-crate under a short alias:
`neutron::html`, `neutron::css`, `neutron::dom`, `neutron::atoms`,
`neutron::layout` (`layout-engine`), `neutron::paint` (`render`),
`neutron::gl` (`webgl`), `neutron::image` (`image_decode`), `neutron::js`
(`js-runtime`), `neutron::quickjs_sys`, `neutron::workers`.

## Why these crates live under `crates/neutron/` and not flat in `crates/`

Cargo has no cross-crate visibility mechanism between separate workspace
members — nesting them doesn't gain compiler-enforced privacy on its own,
the `neutron` facade + boundary-check script do that. The physical nesting
is purely so the three real scopes of this repo line up as three top-level
directories: `crates/atomic` (the GUI shell), `crates/atomicjs`
(experimental JS compiler, its own isolated single-crate workspace), and
`crates/neutron/*` (this pipeline). See
`spec/proposals/NEUTRON_ENCAPSULATION.md` for the full reasoning.

## What's real vs missing per sub-crate

See `spec/matrix/*.md` (JS runtime, DOM, events, CSS/layout,
paint/compositing, browser Web API classes, navigation/security/lifecycle)
and `mockup/rendering-engine-gaps.md` for capability-level detail. This
README is a map, not a status tracker.
