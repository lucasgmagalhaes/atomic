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
            let Some((op, value)) = candidates
                .get(*function_index)
                .and_then(Option::as_ref)
                .and_then(TierOneFunction::unary_const_leaf)
            else {
                continue;
            };
            *instruction = TierOneInstr::InlineUnaryConst { op, value };
        }
    }

    fn unary_const_leaf(&self) -> Option<(NumericOp, f64)> {
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
                Some((*op, *value))
            }
            _ => None,
        }
    }
}
