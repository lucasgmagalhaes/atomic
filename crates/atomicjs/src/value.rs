use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

/// See spec/proposals/ATOMIC_JS_SPIKE.md §5.1. No NaN-boxing, no `Int32` fast
/// path — later-optimization concerns with no place in a spike.
#[derive(Debug, Clone)]
pub enum Value {
    Undefined,
    /// Added while implementing the VM (step 4-6, §8) — not in the original
    /// §5.1 write-up. `LT` needs *some* truthy/falsy representation for
    /// `JUMP_IF_FALSE` to test; representing it as `Number(0.0)`/`Number(1.0)`
    /// instead would silently conflate booleans with numbers (e.g. in a
    /// completion value's `Display` output) for no real savings — a small,
    /// honest addition, same kind of gap as `LoadUpvalue`/`StoreUpvalue`.
    Bool(bool),
    Number(f64),
    Object(Rc<RefCell<JsObject>>),
    Function(Rc<FunctionData>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Undefined => write!(f, "undefined"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Object(_) => write!(f, "[object Object]"),
            Value::Function(_) => write!(f, "[object Function]"),
        }
    }
}

/// No prototypes, no Shapes, no property descriptors — §5.1. This is a
/// compact inline-property representation, deliberately tuned for the small
/// object literals AtomicJS supports: it avoids hashing on every read while
/// preserving insertion-order overwrite semantics. A missing-property read
/// returns `Value::Undefined` (`get`, below), matching real JS semantics.
#[derive(Debug)]
pub struct JsObject {
    /// A stable identity for this object's current property layout. It keeps
    /// inline caches independent from raw pointer identity.
    pub shape_id: u64,
    pub properties: Vec<(String, Value)>,
}

static NEXT_SHAPE_ID: AtomicU64 = AtomicU64::new(1);

impl Default for JsObject {
    fn default() -> Self {
        Self {
            shape_id: NEXT_SHAPE_ID.fetch_add(1, Ordering::Relaxed),
            properties: Vec::new(),
        }
    }
}

impl JsObject {
    pub fn get(&self, name: &str) -> Value {
        self.properties
            .iter()
            .find_map(|(key, value)| (key == name).then(|| value.clone()))
            .unwrap_or(Value::Undefined)
    }

    pub fn get_with_slot(&self, name: &str) -> Option<(usize, Value)> {
        self.properties
            .iter()
            .enumerate()
            .find_map(|(slot, (key, value))| (key == name).then(|| (slot, value.clone())))
    }

    pub fn get_at(&self, slot: usize) -> Value {
        self.properties
            .get(slot)
            .map(|(_, value)| value.clone())
            .unwrap_or(Value::Undefined)
    }

    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        let name = name.into();
        if let Some((_, existing)) = self.properties.iter_mut().find(|(key, _)| *key == name) {
            *existing = value;
        } else {
            self.properties.push((name, value));
            self.shape_id = NEXT_SHAPE_ID.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// A closure: which compiled function to run, plus the upvalue cells it
/// captured at `MAKE_CLOSURE` time (empty for a function that doesn't
/// reference any enclosing local — see `vm.rs`).
#[derive(Debug)]
pub struct FunctionData {
    pub function_index: usize,
    pub captured_env: Vec<Rc<RefCell<Value>>>,
}

/// Optional exact counters exposed by `run_source_with_feedback` for tests
/// and future tiering experiments. Normal `run_source` execution deliberately
/// pays no profiling cost while this spike has no feedback consumer.
#[derive(Debug, Default)]
pub struct FunctionFeedback {
    pub call_count: u32,
    pub loop_count: u32,
}
