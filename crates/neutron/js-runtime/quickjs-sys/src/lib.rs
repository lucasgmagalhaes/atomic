//! Minimal raw FFI bindings to QuickJS-ng, hand-written against the subset
//! of `quickjs.h` this workspace needs (runtime/context lifecycle + eval).
//! Not a full binding — extend as `js-runtime` needs more of the C API.
//!
//! Only covers 64-bit targets (no `JS_NAN_BOXING`, which quickjs.h only
//! enables when `INTPTR_MAX < INT64_MAX`). All three target platforms
//! (Windows/Linux/macOS desktop) build 64-bit, so this is not a limitation
//! in practice — it just means `JSValue`'s layout below would be wrong on
//! a 32-bit target and must not be assumed portable to one.
//!
//! Split into `types.rs` (`JSValue`/tag/calling-convention/class type and
//! constant definitions), `ffi.rs` (the raw `extern "C"` declarations),
//! and `helpers.rs` (`js_null`/`js_undefined`/etc. constant-value
//! constructors) — this file just re-exports their public items so every
//! `quickjs_sys::X` path stays the same as before the split.
#![allow(non_camel_case_types)]

mod ffi;
mod helpers;
mod types;

pub use ffi::*;
pub use helpers::*;
pub use types::*;
