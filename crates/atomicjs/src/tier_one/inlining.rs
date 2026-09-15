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
            let Some(leaf) = candidates
                .get(*function_index)
                .and_then(Option::as_ref)
                .and_then(TierOneFunction::inlineable_leaf)
            else {
                continue;
            };
            *instruction = match (argc, leaf) {
                (1, Leaf::UnaryRight { op, value }) => TierOneInstr::InlineUnaryConst { op, value },
                (1, Leaf::UnaryLeft { value, op }) => TierOneInstr::InlineConstUnary { value, op },
                (2, Leaf::Binary { op }) => TierOneInstr::InlineBinaryArgs { op },
                _ => continue,
            };
        }
    }

    fn inlineable_leaf(&self) -> Option<Leaf> {
        if self.local_count != self.param_count || self.direct_calls().next().is_some() {
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
                        | NumericOp::Lt
                ) =>
            {
                Some(Leaf::UnaryRight {
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
                        | NumericOp::Lt
                ) =>
            {
                Some(Leaf::UnaryLeft {
                    value: *value,
                    op: *op,
                })
            }
            [TierOneInstr::BinaryLocalLocal {
                op,
                left: 0,
                right: 1,
            }, TierOneInstr::Return, ..]
                if self.param_count == 2
                    && matches!(
                        op,
                        NumericOp::Add
                            | NumericOp::Sub
                            | NumericOp::Mul
                            | NumericOp::Div
                            | NumericOp::Mod
                            | NumericOp::Lt
                    ) =>
            {
                Some(Leaf::Binary { op: *op })
            }
            _ => None,
        }
    }
}

enum Leaf {
    UnaryRight { op: NumericOp, value: f64 },
    UnaryLeft { value: f64, op: NumericOp },
    Binary { op: NumericOp },
}
