# AtomicJS — Milestone 13: Shape-guarded property inline cache

## Status

Completed.

## Objective

Remove repeated property-name scans from hot direct member reads while keeping
the VM isolated, deterministic, and correct when a local is assigned an object
with a different layout.

## Delivered

- Added a monotonic layout identity to `JsObject`; adding a property creates a
  new shape identity, while replacing an existing value keeps its slot valid.
- Added per-bytecode-site property caches containing a shape identity and a
  property slot.
- Enabled cache storage only for functions that contain direct local or
  upvalue property-read instructions, and create it anew on every execution.
- Added regression coverage that changes `player` from `{ damage: 20 }` to
  `{ bonus: 1, damage: 7 }` at the same member-read site.

## Result

The longer focused Criterion run on 2026-09-15 measured the stable
`props/run_compiled` case at 21.075 ms median, a 4.37% improvement against the
saved pre-change baseline (95% interval: -6.45% to -2.23%, p < 0.05).
`props/compile_and_run` was inconclusive on the shared machine, so this result
is intentionally limited to the compiled execution path that the cache
targets.

`cargo test --manifest-path crates/atomicjs/Cargo.toml` passes all 69 tests,
and `cargo clippy --manifest-path crates/atomicjs/Cargo.toml --lib -- -D
warnings` passes for AtomicJS.

## Scope boundary

The cache is interpreter-only. It introduces no public API, no application
runtime integration, and no pointer-identity assumption; its guards are object
layout identities and each `CompiledProgram::run` receives fresh cache state.
