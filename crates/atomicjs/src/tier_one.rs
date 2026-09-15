//! @spec atomicjs-profiling#tier-one-executor
//! Numeric Tier 1 bytecode. It deliberately accepts a narrow, proven-safe
//! subset and returns `None` at compile time for every other function; the
//! generic interpreter remains the semantic authority for those functions.

use crate::bytecode::{BytecodeFunction, Const, Instr, NumericOp};
use crate::value::Value;

mod inlining;
mod scratch;
mod value;
use scratch::TierScratch;
use value::{numeric, TierValue};

#[derive(Debug, Clone)]
pub struct TierOneFunction {
    param_count: usize,
    local_count: usize,
    code: Vec<TierOneInstr>,
}

#[derive(Debug, Clone)]
enum TierOneInstr {
    Const(f64),
    Undefined,
    Load(u32),
    Store(u32),
    AddLocalLocal {
        target: u32,
        value: u32,
    },
    AddLocalConst {
        target: u32,
        value: f64,
    },
    BinaryLocalLocal {
        op: NumericOp,
        left: u32,
        right: u32,
    },
    BinaryLocalConst {
        op: NumericOp,
        local: u32,
        value: f64,
    },
    IncrementLocal(u32),
    Numeric(NumericOp),
    CallDirect {
        function_index: usize,
        argc: usize,
    },
    /// A one-argument numeric leaf folded into its caller during Tier-1
    /// candidate preparation. It has no call frame or argument buffer.
    InlineUnaryConst {
        op: NumericOp,
        value: f64,
    },
    /// A one-argument numeric leaf whose constant is the left operand.
    InlineConstUnary {
        value: f64,
        op: NumericOp,
    },
    Jump(usize),
    JumpIfFalse(usize),
    Return,
    Pop,
}

pub struct TierOneResult {
    pub value: Value,
    pub loop_backedges: u32,
}

impl TierOneFunction {
    pub fn compile(function: &BytecodeFunction) -> Option<Self> {
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
        })
    }

    pub fn run(&self, args: &[Value]) -> Option<TierOneResult> {
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
        functions: &[Option<TierOneFunction>],
    ) -> Option<TierOneResult> {
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
