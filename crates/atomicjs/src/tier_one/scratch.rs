//! @spec atomicjs-profiling#tier-one-inlining
//! Reusable numeric frame storage for the specialized executor.

#[derive(Default)]
pub(super) struct TierScratch {
    args: Vec<Vec<f64>>,
    locals: Vec<Vec<f64>>,
}

impl TierScratch {
    pub(super) fn take_args(&mut self) -> Vec<f64> {
        self.args.pop().unwrap_or_default()
    }

    pub(super) fn return_args(&mut self, mut args: Vec<f64>) {
        args.clear();
        self.args.push(args);
    }

    pub(super) fn take_locals(&mut self, local_count: usize) -> Vec<f64> {
        let mut locals = self.locals.pop().unwrap_or_default();
        locals.clear();
        locals.resize(local_count, 0.0);
        locals
    }

    pub(super) fn return_locals(&mut self, mut locals: Vec<f64>) {
        locals.clear();
        self.locals.push(locals);
    }
}
