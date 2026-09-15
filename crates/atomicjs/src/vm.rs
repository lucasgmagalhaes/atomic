//! @spec atomicjs-profiling#modular-execution-core
//! Public VM entry points and compiled-program lifecycle.

use crate::bytecode::BytecodeModule;
use crate::error::AtomicJsError;
use crate::tier_one::TierOneFunction;
use crate::value::{FunctionFeedback, Value};

mod calls;
mod execution;
mod feedback;
mod object;
mod operations;
mod state;
mod tiered;

use execution::execute_function;
use feedback::{CollectingFeedback, NoFeedback, TierOneState};
use state::VmPools;
pub use tiered::{TieredProgram, TieredRun};

/// Compiles and runs one AtomicJS source string.
pub fn run_source(source: &str) -> Result<Value, AtomicJsError> {
    CompiledProgram::compile(source)?.run()
}

/// Immutable bytecode compiled from one AtomicJS source string. Each
/// [`Self::run`] starts a new top-level frame, so state never leaks across
/// executions.
#[derive(Debug)]
pub struct CompiledProgram {
    pub(super) module: BytecodeModule,
}

impl CompiledProgram {
    /// Parses and compiles `source` once for repeated isolated executions.
    pub fn compile(source: &str) -> Result<Self, AtomicJsError> {
        Ok(Self {
            module: compile_module(source)?,
        })
    }

    /// Executes bytecode with fresh VM state and no profiling instrumentation.
    pub fn run(&self) -> Result<Value, AtomicJsError> {
        let pools = VmPools::default();
        let mut feedback = NoFeedback;
        let mut tier_one_calls = 0;
        let mut tier_one_fallbacks = 0;
        let mut tier_one = TierOneState {
            functions: None,
            calls: &mut tier_one_calls,
            fallbacks: &mut tier_one_fallbacks,
        };
        execute_function(
            &self.module,
            self.module.top_level,
            &[],
            &[],
            &mut feedback,
            &pools,
            &mut tier_one,
        )
    }

    /// Executes with per-function feedback, keeping instrumentation opt-in.
    pub fn run_with_feedback(&self) -> Result<(Value, Vec<FunctionFeedback>), AtomicJsError> {
        let mut tier_one_calls = 0;
        let mut tier_one_fallbacks = 0;
        self.run_with_feedback_and_tier_one(None, &mut tier_one_calls, &mut tier_one_fallbacks)
    }

    pub(super) fn run_with_feedback_and_tier_one(
        &self,
        tier_one: Option<&[Option<TierOneFunction>]>,
        tier_one_calls: &mut u32,
        tier_one_fallbacks: &mut u32,
    ) -> Result<(Value, Vec<FunctionFeedback>), AtomicJsError> {
        let mut feedback: Vec<FunctionFeedback> = (0..self.module.functions.len())
            .map(|_| FunctionFeedback::default())
            .collect();
        let pools = VmPools::default();
        let mut collector = CollectingFeedback {
            entries: &mut feedback,
        };
        let mut tier_one = TierOneState {
            functions: tier_one,
            calls: tier_one_calls,
            fallbacks: tier_one_fallbacks,
        };
        let value = execute_function(
            &self.module,
            self.module.top_level,
            &[],
            &[],
            &mut collector,
            &pools,
            &mut tier_one,
        )?;
        Ok((value, feedback))
    }
}

/// Same as [`run_source`], but returns per-function feedback for this run.
pub fn run_source_with_feedback(
    source: &str,
) -> Result<(Value, Vec<FunctionFeedback>), AtomicJsError> {
    CompiledProgram::compile(source)?.run_with_feedback()
}

fn compile_module(source: &str) -> Result<BytecodeModule, AtomicJsError> {
    let tokens = crate::lexer::tokenize(source).map_err(|e| AtomicJsError(e.0))?;
    let program = crate::parser::parse(tokens).map_err(|e| AtomicJsError(e.0))?;
    crate::compiler::compile(program).map_err(|e| AtomicJsError(e.0))
}
