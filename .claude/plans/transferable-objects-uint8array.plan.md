# Plan: Transferable Objects — Real `Uint8Array` Support (ROADMAP item 32)

**Source**: `spec/ROADMAP.md` item 32 (previously marked blocked — "no real `ArrayBuffer`/typed-array type yet"; this session's grounding research found that premise stale: `ArrayBuffer`/`Uint8Array` already exist as real quickjs-ng built-in objects, and `js-runtime` already binds `JS_GetUint8Array`/`JS_NewUint8ArrayCopy`/`JS_NewArrayBufferCopy`)
**Complexity**: Medium

## Summary

Give `structuredClone`/`postMessage`/`IndexedDB` real `Uint8Array` support (the one typed-array kind this codebase already narrows every binary API to, e.g. `crypto.getRandomValues`), and make `postMessage`'s second `transfer` argument real: a transferred `Uint8Array`'s backing buffer is detached on the sending side after the clone, matching the spec's actual observable behavior (the source becomes unusable — `byteLength` 0 — not just "cloned and left alone"). This closes item 32's own documented scope cut in `message_channel.rs`.

Scoped narrower than the full spec on purpose, consistent with `crypto.rs`'s own existing precedent ("the spec allows any integer TypedArray; the other element sizes are a documented narrower scope"): only a real `Uint8Array` is recognized — not a bare `ArrayBuffer` with no view, and not `Int32Array`/`Float64Array`/etc.

## Patterns to Mirror

| Category | Source | Pattern |
|---|---|---|
| Uint8Array extraction | `crates/js-runtime/src/crypto.rs`'s `getRandomValues` | `sys::JS_GetUint8Array(ctx, &mut size, val)` returning a null pointer for anything that isn't exactly a `Uint8Array` — the same narrow-scope detection this plan reuses, not a new check. |
| Structured-clone bridge | `crates/js-runtime/src/value_bridge.rs`'s `js_to_storage_value`/`storage_value_to_js` | Add one more `match` arm each, same shape as the existing `JS_TAG_STRING`/array/object arms — no new dispatch mechanism. |
| Wire-format variant | `crates/storage/src/value/mod.rs`'s `Value` enum + `write.rs`/`parse.rs` | A new `Value::Bytes(Vec<u8>)` variant, encoded as a distinct token (`b"<base64>"`) in the existing hand-rolled JSON-like text format — same "one match arm per variant" shape `write_value`/`parse_value` already have. |
| FFI binding style | `crates/js-runtime/quickjs-sys/src/ffi.rs` | Plain `extern "C" { pub fn ...; }` additions, same as every other binding in this file. |
| Tests | `crates/storage/tests/value_test.rs`, `crates/js-runtime/tests/*.rs` | Round-trip encode/decode test for the new variant; `Context::eval`-based integration tests for `structuredClone`/`postMessage` behavior. |

## Files to Change

| File | Action | Why |
|---|---|---|
| `crates/js-runtime/quickjs-sys/src/ffi.rs` | UPDATE | Add `JS_DetachArrayBuffer`, `JS_GetTypedArrayBuffer` — needed to get a `Uint8Array`'s underlying `ArrayBuffer` and detach it for real transfer semantics. `JS_GetUint8Array`/`JS_NewUint8ArrayCopy`/`JS_NewArrayBufferCopy` are already bound. |
| `crates/storage/Cargo.toml` | UPDATE | Add `base64` (already a workspace dependency elsewhere, e.g. `net`) — the wire encoding for `Value::Bytes`. |
| `crates/storage/src/value/mod.rs` | UPDATE | Add `Value::Bytes(Vec<u8>)`, an `as_bytes()` accessor, and update the module doc's "deviations" list (typed arrays are now real, narrowed to this one variant). |
| `crates/storage/src/value/write.rs` | UPDATE | Encode `Bytes` as `b"<base64>"`. |
| `crates/storage/src/value/parse.rs` | UPDATE | Parse a leading `b"` token into `Value::Bytes` (base64-decoded), alongside the existing `null`/`true`/`false`/string/array/object/number dispatch. |
| `crates/js-runtime/src/value_bridge.rs` | UPDATE | `js_to_storage_value`'s `JS_TAG_OBJECT` arm tries `JS_GetUint8Array` before falling to array/object dispatch; `storage_value_to_js` gets a `Value::Bytes` arm using `JS_NewUint8ArrayCopy`. |
| `crates/js-runtime/src/message_channel.rs` | UPDATE | `port_post_message` reads a real transfer list (`argv[1]`, if a real array): for each element that's a real `Uint8Array`, detach its backing buffer (`JS_GetTypedArrayBuffer` + `JS_DetachArrayBuffer`) *after* `deep_clone` has already captured its bytes. |
| `crates/storage/tests/value_test.rs` | UPDATE | Round-trip test for `Value::Bytes` through `to_wire`/`parse`. |
| `crates/js-runtime/tests/uint8array_transfer_test.rs` | CREATE | `structuredClone` preserves real byte content through a fresh `Uint8Array`; `postMessage` with a transferred `Uint8Array` leaves the source detached (`byteLength === 0`) after the peer receives its own independent copy. |
| `spec/ROADMAP.md` | UPDATE | Flip item 32 to `[~]` with the real scope cut, correcting the stale "blocked" framing. |

## Tasks

### Task 1: FFI bindings for detach + typed-array-to-buffer
- **Action**: Add `JS_DetachArrayBuffer(ctx, obj)` and `JS_GetTypedArrayBuffer(ctx, obj, pbyte_offset, pbyte_length, pbytes_per_element) -> JSValue` to `ffi.rs`, matching quickjs.h's exact signatures.
- **Mirror**: Existing `ffi.rs` declaration style.
- **Validate**: `cargo build -p quickjs-sys`.

### Task 2: `Value::Bytes` wire format
- **Action**: Add `base64` to `storage/Cargo.toml`; add `Value::Bytes(Vec<u8>)` + `as_bytes()`; `write_value` emits `b"<base64 via base64::engine::general_purpose::STANDARD>"`; `parse_value` recognizes a leading `b` followed by `"` as this token (anything else starting with `b` is still a parse error, matching this format's existing "no bare identifiers" strictness).
- **Mirror**: Every existing `Value` variant's own one-line `write_value`/`parse_value` arm.
- **Validate**: `cargo test -p storage --release -- --test-threads=1` (new round-trip test: empty bytes, arbitrary bytes including `\0`/non-UTF8, nested inside an `Array`/`Object`).

### Task 3: Real `Uint8Array` in the structured-clone bridge
- **Action**: In `js_to_storage_value`'s `JS_TAG_OBJECT` arm, call `JS_GetUint8Array` first; a non-null pointer means a real `Uint8Array` — copy its bytes (the pointer is borrowed, per this binding's own doc) into `Value::Bytes`, otherwise fall through to the existing array/object dispatch unchanged. `storage_value_to_js` adds a `Value::Bytes(bytes) => sys::JS_NewUint8ArrayCopy(ctx, bytes.as_ptr(), bytes.len())` arm.
- **Mirror**: `crypto.rs`'s own `JS_GetUint8Array` call site for the extraction half; the existing `Value::Array`/`Value::Object` arms' shape for the reconstruction half.
- **Validate**: `cargo build -p js-runtime` (proven end to end in Task 5).

### Task 4: Real `postMessage` transfer (detach on send)
- **Action**: `port_post_message` reads `argv[1]` when `argc >= 2` and it's a real array (`JS_IsArray`); for each element, `JS_GetUint8Array` to confirm it's a real `Uint8Array`, then `JS_GetTypedArrayBuffer` to get its backing `ArrayBuffer` and `JS_DetachArrayBuffer` it — done *after* `deep_clone(ctx, data)` has already run, so the clone captures the real pre-transfer bytes before the source is detached. A transfer-list entry that isn't a real `Uint8Array` is silently skipped (matches this crate's existing "narrower scope, not a thrown error for every spec edge case" convention — a full spec-compliant version would throw `DataCloneError` for an invalid transfer target).
- **Mirror**: `port_post_message`'s own existing structure — this only adds a step between `deep_clone` and the existing `registry.pending.push(...)`.
- **Validate**: New test in Task 5.

### Task 5: Tests
- **Action**: `crates/storage/tests/value_test.rs`: `Value::Bytes` round-trips through `to_wire`/`parse`, including inside nested `Array`/`Object`. New `crates/js-runtime/tests/uint8array_transfer_test.rs`: (a) `structuredClone(new Uint8Array([1,2,3]))` produces a real, independent `Uint8Array` with the same bytes (mutating the clone doesn't affect the original, proving it's a real copy not a duped reference); (b) `postMessage(new Uint8Array([1,2,3]), [buf])` (via a real `MessageChannel` pair) delivers the bytes to the peer *and* leaves the sender's own `Uint8Array` with `byteLength === 0` afterward; (c) `postMessage` with no transfer list still clones a `Uint8Array` without detaching the source (proving detach is opt-in, not automatic for every `Uint8Array` passed).
- **Mirror**: `crates/js-runtime/tests/*.rs`'s existing `Context::eval`-based assertion style; `window_registry_test.rs`'s own `MessageChannel`-adjacent test shape.
- **Validate**: `cargo test -p storage -p js-runtime --release -- --test-threads=1`.

## Validation

```bash
cargo build -p quickjs-sys -p storage -p js-runtime
cargo test -p storage -p js-runtime --release -- --test-threads=1
cargo build --workspace --exclude shell
cargo test --workspace --exclude shell --exclude automation --release -- --test-threads=1
cargo fmt --all
```

## Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| `JS_GetTypedArrayBuffer`'s returned `ArrayBuffer` `JSValue` is an owned reference that must be freed after `JS_DetachArrayBuffer` uses it, or a refcount leak results (same class of bug this session already hit and fixed once in `window_registry`'s dispatch code) | Medium | Explicitly `JS_FreeValue` the buffer value right after detaching it, matching quickjs.h's normal "returns a new reference" convention for `JS_Get*` calls that return a `JSValue`. |
| A transfer-list entry that's the *same* `Uint8Array` also nested inside `data` gets detached before `deep_clone` reads it, if the ordering is wrong | Medium (real correctness bug if mishandled) | Task 4's own ordering is explicit about this: `deep_clone` must run to completion *before* any detach call — the plan and implementation must not reorder this. |
| Base64 adds a real new dependency to a crate (`storage`) that currently has zero dependencies | Low | `base64` is already a vetted, in-use workspace dependency (`net`, `security`, `import`) — same version pin (`"0.23.1"`), not a new supply-chain surface. |
| Deep nested nested detection: `js_to_storage_value`'s `JS_GetUint8Array` check must run *before* `JS_IsArray` (a `Uint8Array` also satisfies certain array-like checks in some engines) | Low | Verified against quickjs-ng's own `JS_IsArray` doc comment (already read this session: it reflects real `Array.isArray` semantics, which is `false` for a `Uint8Array`) — order doesn't actually matter here, but `JS_GetUint8Array` first is still checked first since it's the narrower, more specific test. |
| No support for a bare `ArrayBuffer` with no view, or other `TypedArray` kinds | High (accepted) | Documented scope cut, matching `crypto.rs`'s own existing narrower-than-spec precedent for exactly this reason. |

## Acceptance
- [ ] Task 1-5 complete, each independently testable
- [ ] `structuredClone`/`postMessage` real-copy a `Uint8Array`'s actual bytes (not `Value::Null`, the current silent-collapse behavior)
- [ ] A transferred `Uint8Array` is really detached (`byteLength === 0`) on the sender's side after `postMessage`
- [ ] Full workspace build + test green
- [ ] `spec/ROADMAP.md` item 32 updated with the real scope cut (Uint8Array only, no bare ArrayBuffer, no other TypedArray kinds) and the "blocked" framing corrected
