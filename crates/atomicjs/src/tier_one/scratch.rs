//! @spec atomicjs-profiling#tier-one-inlining
//! Reusable frame storage for the specialized executor.

use super::value::TierValue;

#[derive(Default)]
pub(crate) struct TierScratch {
    args: Vec<Vec<f64>>,
    locals: Vec<Vec<TierValue>>,
}

impl TierScratch {
    pub(crate) fn take_args(&mut self) -> Vec<f64> {
        self.args.pop().unwrap_or_default()
    }

    pub(crate) fn return_args(&mut self, mut args: Vec<f64>) {
        args.clear();
        self.args.push(args);
    }

    pub(crate) fn take_locals(&mut self, local_count: usize) -> Vec<TierValue> {
        let mut locals = self.locals.pop().unwrap_or_default();
        locals.clear();
        locals.resize(local_count, TierValue::Undefined);
        locals
    }

    pub(crate) fn return_locals(&mut self, mut locals: Vec<TierValue>) {
        locals.clear();
        self.locals.push(locals);
    }
}
