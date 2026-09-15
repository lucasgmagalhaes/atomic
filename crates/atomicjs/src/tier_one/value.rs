//! @spec atomicjs-profiling#tier-one-inlining
//! Numeric value operations used exclusively by Tier 1.

use std::cell::RefCell;
use std::rc::Rc;

use crate::bytecode::NumericOp;
use crate::value::{JsObject, Value};

#[derive(Debug, Clone)]
pub(crate) enum TierValue {
    Undefined,
    Number(f64),
    Bool(bool),
    Object(Rc<RefCell<JsObject>>),
}

impl TierValue {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        match value {
            Value::Undefined => Some(Self::Undefined),
            Value::Number(value) => Some(Self::Number(*value)),
            Value::Bool(value) => Some(Self::Bool(*value)),
            Value::Object(object) => Some(Self::Object(object.clone())),
            _ => None,
        }
    }

    pub(crate) fn into_value(self) -> Value {
        match self {
            Self::Undefined => Value::Undefined,
            Self::Number(value) => Value::Number(value),
            Self::Bool(value) => Value::Bool(value),
            Self::Object(object) => Value::Object(object),
        }
    }
}

pub(crate) fn numeric(op: NumericOp, left: f64, right: f64) -> TierValue {
    match op {
        NumericOp::Add => TierValue::Number(left + right),
        NumericOp::Sub => TierValue::Number(left - right),
        NumericOp::Mul => TierValue::Number(left * right),
        NumericOp::Div => TierValue::Number(left / right),
        NumericOp::Mod => TierValue::Number(left % right),
        NumericOp::Lt => TierValue::Bool(left < right),
    }
}
