//! @spec atomicjs-profiling#tier-one-inlining
//! Numeric value operations used exclusively by Tier 1.

use crate::bytecode::NumericOp;

#[derive(Debug, Clone, Copy)]
pub(super) enum TierValue {
    Undefined,
    Number(f64),
    Bool(bool),
}

pub(super) fn numeric(op: NumericOp, left: f64, right: f64) -> TierValue {
    match op {
        NumericOp::Add => TierValue::Number(left + right),
        NumericOp::Sub => TierValue::Number(left - right),
        NumericOp::Mul => TierValue::Number(left * right),
        NumericOp::Div => TierValue::Number(left / right),
        NumericOp::Mod => TierValue::Number(left % right),
        NumericOp::Lt => TierValue::Bool(left < right),
    }
}
