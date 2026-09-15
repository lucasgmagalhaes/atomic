# AtomicJS — Milestone 12: Minimal QuickJS differential oracle

## Status

Completed.

## Objective

Keep differential correctness testing independent from application runtime
layers and prevent the AtomicJS test/benchmark toolchain from compiling the
entire browser-oriented `js-runtime` crate.

## Delivered

- Replaced the test-only `js-runtime` dependency with the existing local
  `quickjs-sys` binding.
- Evaluated supported programs through the minimal QuickJS-ng lifecycle:
  runtime, context, global eval, result conversion, and explicit cleanup.
- Preserved the same fixed and seeded differential corpus.

## Result

`cargo test --manifest-path crates/atomicjs/Cargo.toml` continues to pass all
68 tests. The benchmark target can now build without linking AtomicJS's
development workflow to DOM, network, storage, layout, or rendering code.

## Scope boundary

`quickjs-sys` is an existing local FFI binding and remains a development-only
oracle. AtomicJS's public API, production dependencies, and runtime behavior
are unchanged.
