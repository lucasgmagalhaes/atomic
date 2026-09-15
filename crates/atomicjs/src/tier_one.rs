//! @spec atomicjs-profiling#tier-one-executor
//! Numeric Tier 1 bytecode. It deliberately accepts a narrow, proven-safe
//! subset and returns `None` at compile time for every other function; the
//! generic interpreter remains the semantic authority for those functions.

use crate::bytecode::{BytecodeFunction, Const, Instr, NumericOp};
use crate::value::Value;
mod execution;
mod guards;
mod inlining;
mod instruction;
mod scratch;
mod value;
use instruction::TierOneInstr;
use scratch::TierScratch;
use value::TierValue;

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
                Instr::GetLocalProp { local, name } => {
                    let Const::String(name) = function.constants.get(*name as usize)? else {
                        return None;
                    };
                    TierOneInstr::GetLocalProp {
                        local: *local,
                        name: name.clone(),
                    }
                }
                Instr::AddLocalProp {
                    target,
                    object,
                    name,
                } => {
                    let Const::String(name) = function.constants.get(*name as usize)? else {
                        return None;
                    };
                    TierOneInstr::AddLocalProp {
                        target: *target,
                        object: *object,
                        name: name.clone(),
                    }
                }
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
        let tier_args: Vec<TierValue> = args
            .iter()
            .map(TierValue::from_value)
            .collect::<Option<_>>()?;
        let mut scratch = TierScratch::default();
        self.run_values(&tier_args, &[], &mut scratch)
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
        if args.len() != self.param_count {
            return None;
        }
        let tier_args: Vec<TierValue> = args
            .iter()
            .map(TierValue::from_value)
            .collect::<Option<_>>()?;
        let mut scratch = TierScratch::default();
        self.run_values(&tier_args, functions, &mut scratch)
    }

    pub(super) fn run_values(
        &self,
        args: &[TierValue],
        functions: &[Option<TierOneFunction>],
        scratch: &mut TierScratch,
    ) -> Option<TierOneResult> {
        execution::run(
            &self.code,
            self.param_count,
            self.local_count,
            args,
            functions,
            scratch,
        )
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
