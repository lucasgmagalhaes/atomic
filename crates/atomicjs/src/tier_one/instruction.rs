//! @spec atomicjs-profiling#tier-one-inlining
//! Specialized bytecode emitted only for numeric Tier-1 candidates.

use crate::bytecode::NumericOp;

#[derive(Debug, Clone)]
pub(super) enum TierOneInstr {
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
    InlineUnaryConst {
        op: NumericOp,
        value: f64,
    },
    InlineConstUnary {
        value: f64,
        op: NumericOp,
    },
    InlineBinaryArgs {
        op: NumericOp,
    },
    Jump(usize),
    JumpIfFalse(usize),
    Return,
    Pop,
}
