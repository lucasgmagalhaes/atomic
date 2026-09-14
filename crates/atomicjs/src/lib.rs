//! atomicjs — validation-spike interpreter. See
//! spec/proposals/ATOMIC_JS_SPIKE.md for scope, methodology, and the
//! step-by-step plan this crate is built against. Deliberately not a member
//! of the root workspace (own empty `[workspace]` stanza in Cargo.toml) —
//! see that doc's §6 for why.

pub mod error;
pub mod lexer;
pub mod value;

pub use error::AtomicJsError;
pub use value::{FunctionData, FunctionFeedback, JsObject, Value};

/// Runs `source` as a script and returns its completion value: the value of
/// the last top-level expression statement evaluated, or `Value::Undefined`
/// if the script's last statement isn't an expression statement (see
/// ATOMIC_JS_SPIKE.md §5.4). This is the only symbol the benchmark harness
/// (§7) calls.
///
/// Step-1 scaffold stub: always returns `Value::Undefined`. Real lexing,
/// parsing, compiling, and execution land in steps 2-6 of §8's plan.
pub fn run_source(_source: &str) -> Result<Value, AtomicJsError> {
    Ok(Value::Undefined)
}
