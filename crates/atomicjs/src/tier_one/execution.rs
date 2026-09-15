//! @spec atomicjs-profiling#tier-one-executor
//! Guarded Tier-1 instruction dispatch.

use crate::value::Value;

use super::guards;
use super::instruction::TierOneInstr;
use super::scratch::TierScratch;
use super::value::{numeric, TierValue};
use super::{TierOneFunction, TierOneResult};

pub(super) fn run(
    code: &[TierOneInstr],
    param_count: usize,
    local_count: usize,
    args: &[TierValue],
    functions: &[Option<TierOneFunction>],
    scratch: &mut TierScratch,
) -> Option<TierOneResult> {
    if args.len() != param_count {
        return None;
    }
    let mut locals = scratch.take_locals(local_count);
    for (slot, value) in args.iter().enumerate() {
        *locals.get_mut(slot)? = value.clone();
    }
    let mut stack = Vec::with_capacity(8);
    let mut pc = 0;
    let mut loop_backedges: u32 = 0;
    let mut property_caches = vec![None; code.len()];
    loop {
        match code.get(pc)? {
            TierOneInstr::Const(value) => stack.push(TierValue::Number(*value)),
            TierOneInstr::Undefined => stack.push(TierValue::Undefined),
            TierOneInstr::Load(slot) => stack.push(locals.get(*slot as usize)?.clone()),
            TierOneInstr::Store(slot) => {
                let TierValue::Number(value) = stack.pop()? else {
                    return None;
                };
                *locals.get_mut(*slot as usize)? = TierValue::Number(value);
            }
            TierOneInstr::AddLocalLocal { target, value } => {
                let TierValue::Number(value) = locals.get(*value as usize)? else {
                    return None;
                };
                let value = *value;
                let TierValue::Number(target) = locals.get_mut(*target as usize)? else {
                    return None;
                };
                *target += value;
            }
            TierOneInstr::AddLocalConst { target, value } => {
                let TierValue::Number(target) = locals.get_mut(*target as usize)? else {
                    return None;
                };
                *target += value;
            }
            TierOneInstr::BinaryLocalLocal { op, left, right } => {
                let TierValue::Number(left) = locals.get(*left as usize)? else {
                    return None;
                };
                let TierValue::Number(right) = locals.get(*right as usize)? else {
                    return None;
                };
                stack.push(numeric(*op, *left, *right));
            }
            TierOneInstr::BinaryLocalConst { op, local, value } => {
                let TierValue::Number(local) = locals.get(*local as usize)? else {
                    return None;
                };
                stack.push(numeric(*op, *local, *value));
            }
            TierOneInstr::IncrementLocal(slot) => {
                let TierValue::Number(value) = locals.get_mut(*slot as usize)? else {
                    return None;
                };
                *value += 1.0;
            }
            TierOneInstr::GetLocalProp { local, name } => {
                stack.push(TierValue::Number(guards::numeric_local_property(
                    &locals,
                    *local as usize,
                    name,
                    property_caches.get_mut(pc)?,
                )?))
            }
            TierOneInstr::AddLocalProp {
                target,
                object,
                name,
            } => {
                let value = guards::numeric_local_property(
                    &locals,
                    *object as usize,
                    name,
                    property_caches.get_mut(pc)?,
                )?;
                let TierValue::Number(target) = locals.get_mut(*target as usize)? else {
                    return None;
                };
                *target += value;
            }
            TierOneInstr::Numeric(op) => {
                let TierValue::Number(right) = stack.pop()? else {
                    return None;
                };
                let TierValue::Number(left) = stack.pop()? else {
                    return None;
                };
                stack.push(numeric(*op, left, right));
            }
            TierOneInstr::Native(native) => {
                let TierValue::Number(value) = stack.pop()? else {
                    return None;
                };
                stack.push(TierValue::Number(match native {
                    crate::bytecode::NativeFn::Sqrt => value.sqrt(),
                    crate::bytecode::NativeFn::Log => value.ln(),
                }));
            }
            TierOneInstr::CallDirect {
                function_index,
                argc,
            } => {
                let start = stack.len().checked_sub(*argc)?;
                let mut child_args = scratch.take_args();
                for value in &stack[start..] {
                    let TierValue::Number(value) = value else {
                        return None;
                    };
                    child_args.push(*value);
                }
                stack.truncate(start);
                let child = functions.get(*function_index)?.as_ref()?;
                let child_values: Vec<TierValue> =
                    child_args.iter().copied().map(TierValue::Number).collect();
                let child_result = child.run_values(&child_values, functions, scratch);
                scratch.return_args(child_args);
                let child_result = child_result?;
                loop_backedges = loop_backedges.saturating_add(child_result.loop_backedges);
                let Value::Number(value) = child_result.value else {
                    return None;
                };
                stack.push(TierValue::Number(value));
            }
            TierOneInstr::InlineUnaryConst { op, value } => {
                let TierValue::Number(argument) = stack.pop()? else {
                    return None;
                };
                stack.push(numeric(*op, argument, *value));
            }
            TierOneInstr::InlineConstUnary { value, op } => {
                let TierValue::Number(argument) = stack.pop()? else {
                    return None;
                };
                stack.push(numeric(*op, *value, argument));
            }
            TierOneInstr::InlineBinaryArgs { op } => {
                let TierValue::Number(right) = stack.pop()? else {
                    return None;
                };
                let TierValue::Number(left) = stack.pop()? else {
                    return None;
                };
                stack.push(numeric(*op, left, right));
            }
            TierOneInstr::Jump(target) => {
                if *target <= pc {
                    loop_backedges = loop_backedges.saturating_add(1);
                }
                pc = *target;
                continue;
            }
            TierOneInstr::JumpIfFalse(target) => {
                let TierValue::Bool(condition) = stack.pop()? else {
                    return None;
                };
                if !condition {
                    pc = *target;
                    continue;
                }
            }
            TierOneInstr::Return => {
                let result = TierOneResult {
                    value: stack.pop()?.into_value(),
                    loop_backedges,
                };
                scratch.return_locals(locals);
                return Some(result);
            }
            TierOneInstr::Pop => {
                stack.pop()?;
            }
        }
        pc += 1;
    }
}
