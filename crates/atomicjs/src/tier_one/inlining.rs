//! @spec atomicjs-profiling#tier-one-inlining
//! Conservative leaf recognition and rewriting for Tier-1 candidates.

use super::*;

impl TierOneFunction {
    pub(crate) fn inline_leaf_calls(&mut self, candidates: &[Option<TierOneFunction>]) {
        for instruction in &mut self.code {
            let TierOneInstr::CallDirect {
                function_index,
                argc,
            } = instruction
            else {
                continue;
            };
            if *argc != 1 {
                continue;
            }
            let Some(leaf) = candidates
                .get(*function_index)
                .and_then(Option::as_ref)
                .and_then(TierOneFunction::unary_const_leaf)
            else {
                continue;
            };
            *instruction = match leaf {
                UnaryConstLeaf::Right { op, value } => TierOneInstr::InlineUnaryConst { op, value },
                UnaryConstLeaf::Left { value, op } => TierOneInstr::InlineConstUnary { value, op },
            };
        }
    }

    fn unary_const_leaf(&self) -> Option<UnaryConstLeaf> {
        if self.param_count != 1 || self.local_count != 1 || self.direct_calls().next().is_some() {
            return None;
        }
        match self.code.as_slice() {
            [TierOneInstr::BinaryLocalConst {
                op,
                local: 0,
                value,
            }, TierOneInstr::Return, ..]
                if matches!(
                    op,
                    NumericOp::Add
                        | NumericOp::Sub
                        | NumericOp::Mul
                        | NumericOp::Div
                        | NumericOp::Mod
                ) =>
            {
                Some(UnaryConstLeaf::Right {
                    op: *op,
                    value: *value,
                })
            }
            [TierOneInstr::Const(value), TierOneInstr::Load(0), TierOneInstr::Numeric(op), TierOneInstr::Return, ..]
                if matches!(
                    op,
                    NumericOp::Add
                        | NumericOp::Sub
                        | NumericOp::Mul
                        | NumericOp::Div
                        | NumericOp::Mod
                ) =>
            {
                Some(UnaryConstLeaf::Left {
                    value: *value,
                    op: *op,
                })
            }
            _ => None,
        }
    }
}

enum UnaryConstLeaf {
    Right { op: NumericOp, value: f64 },
    Left { value: f64, op: NumericOp },
}
