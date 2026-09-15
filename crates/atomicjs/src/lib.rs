//! atomicjs — validation-spike interpreter. See
//! spec/proposals/ATOMIC_JS_SPIKE.md for scope, methodology, and the
//! step-by-step plan this crate is built against. Deliberately not a member
//! of the root workspace (own empty `[workspace]` stanza in Cargo.toml) —
//! see that doc's §6 for why.

pub mod ast;
pub mod bytecode;
pub mod compiler;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod value;
pub mod vm;

pub use error::AtomicJsError;
pub use value::{FunctionData, FunctionFeedback, JsObject, Value};
pub use vm::{run_source, run_source_with_feedback, CompiledProgram};
