use std::fmt;

/// Shared error type for the parser (syntax errors) and the VM (runtime
/// errors) — see spec/proposals/ATOMIC_JS_SPIKE.md §5.1. Kept deliberately
/// minimal: the spike's hard scope limits (§3) exclude a real try/catch/Error
/// object model, so this only needs to carry a message.
#[derive(Debug)]
pub struct AtomicJsError(pub String);

impl fmt::Display for AtomicJsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AtomicJsError {}
