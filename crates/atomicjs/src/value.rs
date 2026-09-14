use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// See spec/proposals/ATOMIC_JS_SPIKE.md §5.1. No NaN-boxing, no `Int32` fast
/// path — later-optimization concerns with no place in a spike.
#[derive(Debug, Clone)]
pub enum Value {
    Undefined,
    Number(f64),
    Object(Rc<RefCell<JsObject>>),
    Function(Rc<FunctionData>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Undefined => write!(f, "undefined"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Object(_) => write!(f, "[object Object]"),
            Value::Function(_) => write!(f, "[object Function]"),
        }
    }
}

/// No prototypes, no Shapes, no property descriptors — §5.1. A
/// missing-property read returns `Value::Undefined` (`get`, below), matching
/// real JS semantics; this is what `GET_PROP` (§5.3) relies on.
#[derive(Debug, Default)]
pub struct JsObject {
    pub properties: HashMap<String, Value>,
}

impl JsObject {
    pub fn get(&self, name: &str) -> Value {
        self.properties
            .get(name)
            .cloned()
            .unwrap_or(Value::Undefined)
    }

    pub fn set(&mut self, name: impl Into<String>, value: Value) {
        self.properties.insert(name.into(), value);
    }
}

/// Populated starting with the bytecode/compiler/VM steps (§8 steps 3-6) —
/// left as a placeholder for the step-1 scaffold checkpoint.
#[derive(Debug)]
pub struct FunctionData {}

/// Cheap, sampled-not-per-opcode counters a future tiering orchestrator would
/// need (see ATOMIC_JS_TIERING.md §3) — defined now so `vm.rs` can increment
/// them starting in step 4, without this spike building any consumer of the
/// data. Nothing reads this in the spike itself.
#[derive(Debug, Default)]
pub struct FunctionFeedback {
    pub call_count: u32,
    pub loop_count: u32,
}
