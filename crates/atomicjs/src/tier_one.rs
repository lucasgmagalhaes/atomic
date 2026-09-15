//! @spec atomicjs-profiling#tier-one-executor
//! Numeric Tier 1 bytecode. It deliberately accepts a narrow, proven-safe
//! subset and returns `None` at compile time for every other function; the
//! generic interpreter remains the semantic authority for those functions.

use crate::bytecode::{BytecodeFunction, Const, Instr, NumericOp};
use crate::value::Value;

#[derive(Debug, Clone, Copy)]
enum TierValue {
    Undefined,
    Number(f64),
    Bool(bool),
}

#[derive(Debug, Clone)]
pub struct TierOneFunction {
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
        if !function.captured_locals.is_empty() {
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
                Instr::Jump(target) => TierOneInstr::Jump(*target),
                Instr::JumpIfFalse(target) => TierOneInstr::JumpIfFalse(*target),
                Instr::Return => TierOneInstr::Return,
                Instr::Pop => TierOneInstr::Pop,
                _ => return None,
            };
            code.push(lowered);
        }
        Some(Self {
            local_count: function.local_count,
            code,
        })
    }

    pub fn run(&self, args: &[Value]) -> Option<TierOneResult> {
        let mut locals = vec![0.0; self.local_count];
        for (slot, value) in args.iter().enumerate() {
            let Value::Number(value) = value else {
                return None;
            };
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
                    return Some(TierOneResult {
                        value: match stack.pop()? {
                            TierValue::Undefined => Value::Undefined,
                            TierValue::Number(value) => Value::Number(value),
                            TierValue::Bool(value) => Value::Bool(value),
                        },
                        loop_backedges,
                    })
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
}

fn numeric(op: NumericOp, left: f64, right: f64) -> TierValue {
    match op {
        NumericOp::Add => TierValue::Number(left + right),
        NumericOp::Sub => TierValue::Number(left - right),
        NumericOp::Mul => TierValue::Number(left * right),
        NumericOp::Div => TierValue::Number(left / right),
        NumericOp::Mod => TierValue::Number(left % right),
        NumericOp::Lt => TierValue::Bool(left < right),
    }
}
