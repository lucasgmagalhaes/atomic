//! @spec atomicjs-profiling#tier-one-executor
//! Numeric Tier 1 bytecode. It deliberately accepts a narrow, proven-safe
//! subset and returns `None` at compile time for every other function; the
//! generic interpreter remains the semantic authority for those functions.

use crate::bytecode::{BytecodeFunction, Const, Instr, NumericOp};
use crate::value::Value;
mod guards;
mod inlining;
mod instruction;
mod scratch;
mod value;
use instruction::TierOneInstr;
use scratch::TierScratch;
use value::{numeric, TierValue};

#[derive(Debug, Clone)]
pub struct TierOneFunction {
    param_count: usize,
    local_count: usize,
    code: Vec<TierOneInstr>,
    property_read: Option<(usize, String)>,
    increment_upvalue: Option<usize>,
}

pub struct TierOneResult {
    pub value: Value,
    pub loop_backedges: u32,
}

impl TierOneFunction {
    pub fn compile(function: &BytecodeFunction) -> Option<Self> {
        let property_read = match function.code.as_slice() {
            [Instr::GetLocalProp { local, name }, Instr::Return, ..] => {
                match function.constants.get(*name as usize)? {
                    Const::String(name) => Some((*local as usize, name.clone())),
                    _ => return None,
                }
            }
            _ => None,
        };
        let increment_upvalue = match function.code.as_slice() {
            [Instr::IncrementUpvalue(index), Instr::Return, ..] => Some(*index as usize),
            _ => None,
        };
        if property_read.is_some() || increment_upvalue.is_some() {
            return Some(Self {
                param_count: function.param_count,
                local_count: function.local_count,
                code: Vec::new(),
                property_read,
                increment_upvalue,
            });
        }
        if !function.captured_locals.is_empty() || function.upvalue_count != 0 {
            return None;
        }
        let mut code = Vec::with_capacity(function.code.len());
        for instruction in &function.code {
            let lowered = match instruction {
                Instr::LoadConst(index) => match function.constants.get(*index as usize)? {
                    Const::Number(value) => TierOneInstr::Const(*value),
                    Const::Undefined => TierOneInstr::Undefined,
                    Const::String(_) => return None,
                },
                Instr::LoadLocal(slot) => TierOneInstr::Load(*slot),
                Instr::StoreLocal(slot) => TierOneInstr::Store(*slot),
                Instr::AddLocalLocal { target, value } => TierOneInstr::AddLocalLocal {
                    target: *target,
                    value: *value,
                },
                Instr::AddLocalConst { target, constant } => {
                    let Const::Number(value) = function.constants.get(*constant as usize)? else {
                        return None;
                    };
                    TierOneInstr::AddLocalConst {
                        target: *target,
                        value: *value,
                    }
                }
                Instr::BinaryLocalLocal { op, left, right } => TierOneInstr::BinaryLocalLocal {
                    op: *op,
                    left: *left,
                    right: *right,
                },
                Instr::BinaryLocalConst {
                    op,
                    local,
                    constant,
                } => {
                    let Const::Number(value) = function.constants.get(*constant as usize)? else {
                        return None;
                    };
                    TierOneInstr::BinaryLocalConst {
                        op: *op,
                        local: *local,
                        value: *value,
                    }
                }
                Instr::IncrementLocal(slot) => TierOneInstr::IncrementLocal(*slot),
                Instr::Add => TierOneInstr::Numeric(NumericOp::Add),
                Instr::Sub => TierOneInstr::Numeric(NumericOp::Sub),
                Instr::Mul => TierOneInstr::Numeric(NumericOp::Mul),
                Instr::Div => TierOneInstr::Numeric(NumericOp::Div),
                Instr::Mod => TierOneInstr::Numeric(NumericOp::Mod),
                Instr::Lt => TierOneInstr::Numeric(NumericOp::Lt),
                Instr::CallNative(native) => TierOneInstr::Native(*native),
                Instr::CallDirect {
                    function_index,
                    argc,
                } => TierOneInstr::CallDirect {
                    function_index: *function_index as usize,
                    argc: *argc as usize,
                },
                Instr::Jump(target) => TierOneInstr::Jump(*target),
                Instr::JumpIfFalse(target) => TierOneInstr::JumpIfFalse(*target),
                Instr::Return => TierOneInstr::Return,
                Instr::Pop => TierOneInstr::Pop,
                _ => return None,
            };
            code.push(lowered);
        }
        Some(Self {
            param_count: function.param_count,
            local_count: function.local_count,
            code,
            property_read: None,
            increment_upvalue: None,
        })
    }

    pub fn run(&self, args: &[Value]) -> Option<TierOneResult> {
        if let Some((local, name)) = &self.property_read {
            return guards::property_read(args, self.param_count, *local, name);
        }
        if args.len() != self.param_count {
            return None;
        }
        if !args.iter().all(|value| matches!(value, Value::Number(_))) {
            return None;
        }
        let numeric_args: Vec<f64> = args
            .iter()
            .map(|value| match value {
                Value::Number(value) => *value,
                _ => unreachable!(),
            })
            .collect();
        let mut scratch = TierScratch::default();
        self.run_numeric(&numeric_args, &[], &mut scratch)
    }

    pub(crate) fn run_with_functions(
        &self,
        args: &[Value],
        upvalues: &[std::rc::Rc<std::cell::RefCell<Value>>],
        functions: &[Option<TierOneFunction>],
    ) -> Option<TierOneResult> {
        if let Some((local, name)) = &self.property_read {
            return guards::property_read(args, self.param_count, *local, name);
        }
        if let Some(index) = self.increment_upvalue {
            return guards::increment_upvalue(upvalues, index);
        }
        if args.len() != self.param_count
            || !args.iter().all(|value| matches!(value, Value::Number(_)))
        {
            return None;
        }
        let numeric_args: Vec<f64> = args
            .iter()
            .map(|value| match value {
                Value::Number(value) => *value,
                _ => unreachable!(),
            })
            .collect();
        let mut scratch = TierScratch::default();
        self.run_numeric(&numeric_args, functions, &mut scratch)
    }

    fn run_numeric(
        &self,
        args: &[f64],
        functions: &[Option<TierOneFunction>],
        scratch: &mut TierScratch,
    ) -> Option<TierOneResult> {
        if args.len() != self.param_count {
            return None;
        }
        let mut locals = scratch.take_locals(self.local_count);
        for (slot, value) in args.iter().enumerate() {
            *locals.get_mut(slot)? = *value;
        }
        let mut stack = Vec::with_capacity(8);
        let mut pc = 0;
        let mut loop_backedges: u32 = 0;
        loop {
            match self.code.get(pc)? {
                TierOneInstr::Const(value) => stack.push(TierValue::Number(*value)),
                TierOneInstr::Undefined => stack.push(TierValue::Undefined),
                TierOneInstr::Load(slot) => {
                    stack.push(TierValue::Number(*locals.get(*slot as usize)?))
                }
                TierOneInstr::Store(slot) => {
                    let TierValue::Number(value) = stack.pop()? else {
                        return None;
                    };
                    *locals.get_mut(*slot as usize)? = value;
                }
                TierOneInstr::AddLocalLocal { target, value } => {
                    *locals.get_mut(*target as usize)? += *locals.get(*value as usize)?;
                }
                TierOneInstr::AddLocalConst { target, value } => {
                    *locals.get_mut(*target as usize)? += value;
                }
                TierOneInstr::BinaryLocalLocal { op, left, right } => {
                    stack.push(numeric(
                        *op,
                        *locals.get(*left as usize)?,
                        *locals.get(*right as usize)?,
                    ));
                }
                TierOneInstr::BinaryLocalConst { op, local, value } => {
                    stack.push(numeric(*op, *locals.get(*local as usize)?, *value));
                }
                TierOneInstr::IncrementLocal(slot) => *locals.get_mut(*slot as usize)? += 1.0,
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
                    let child_result = child.run_numeric(&child_args, functions, scratch);
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
                        value: match stack.pop()? {
                            TierValue::Undefined => Value::Undefined,
                            TierValue::Number(value) => Value::Number(value),
                            TierValue::Bool(value) => Value::Bool(value),
                        },
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

    pub fn estimated_bytes(&self) -> usize {
        self.code
            .len()
            .saturating_mul(std::mem::size_of::<TierOneInstr>())
    }

    pub(crate) fn direct_calls(&self) -> impl Iterator<Item = usize> + '_ {
        self.code
            .iter()
            .filter_map(|instruction| match instruction {
                TierOneInstr::CallDirect { function_index, .. } => Some(*function_index),
                _ => None,
            })
    }
}
