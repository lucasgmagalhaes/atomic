# atomicjs

Experimental JS compiler/VM — a research spike, not the browser's
production JS engine. That's `neutron::js` (`crates/neutron/js-runtime`,
wrapping quickjs-ng). This crate exists to explore whether a
Rust-native, tiered (interpreter → optimizing "tier one") JS engine can
beat quickjs-ng on this project's actual workload (many lightweight
profiles idling/running small scripts), not to replace it outright.

## Layout

`lexer.rs` → `parser.rs`/`parser/` → `ast.rs` → `compiler.rs`/`compiler/`
→ `bytecode.rs` → `vm.rs`/`vm/` (bytecode interpreter) → `tier_one.rs`/
`tier_one/` (the optimizing tier, promoted to via `tiering.rs`'s
feedback-driven heuristics). `test262.rs` runs the official ECMAScript
conformance suite against it.

## Why it's isolated

`Cargo.toml` declares its own empty-members `[workspace]` — that makes
`crates/atomicjs` the root of its own single-crate workspace, deliberately
decoupled from the repo's root `Cargo.toml`. `cargo build --workspace` /
`cargo test --workspace` from the repo root never touch this crate while
its thesis is unproven (see `spec/proposals/ATOMIC_JS_SPIKE.md` §6 for the
full reasoning). Build/bench/test it standalone, from inside this
directory:

```
cd crates/atomicjs
cargo test
cargo bench
```

See `spec/proposals/ATOMIC_JS_*.md` for the spike's roadmap, milestones,
tiering design, and ECMAScript-conformance tracking.
