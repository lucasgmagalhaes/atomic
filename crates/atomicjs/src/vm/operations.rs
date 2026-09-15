//! @spec atomicjs-profiling#modular-execution-core
//! Primitive numeric and boolean operations used by bytecode dispatch.

use crate::bytecode::NumericOp;
use crate::value::Value;

pub(super) fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Undefined => false,
        Value::Bool(value) => *value,
        Value::Number(value) => *value != 0.0,
        Value::Object(_) | Value::Function(_) => true,
    }
}

pub(super) fn as_number(value: &Value) -> f64 {
    match value {
        Value::Number(value) => *value,
        other => panic!("internal error: expected Number, got {other:?}"),
    }
}

pub(super) fn binary_number(stack: &mut Vec<Value>, operation: impl FnOnce(f64, f64) -> f64) {
    let right = stack.pop().expect("binary operation needs two operands");
    let left = stack.pop().expect("binary operation needs two operands");
    stack.push(Value::Number(operation(
        as_number(&left),
        as_number(&right),
    )));
}

pub(super) fn numeric_value(operation: NumericOp, left: f64, right: f64) -> Value {
    match operation {
        NumericOp::Add => Value::Number(left + right),
        NumericOp::Sub => Value::Number(left - right),
        NumericOp::Mul => Value::Number(left * right),
        NumericOp::Div => Value::Number(left / right),
        NumericOp::Mod => Value::Number(left % right),
        NumericOp::Lt => Value::Bool(left < right),
    }
}
