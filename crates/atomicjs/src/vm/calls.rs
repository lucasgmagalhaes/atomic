//! @spec atomicjs-profiling#modular-execution-core
//! Function-call fast paths shared by bytecode dispatch.

use crate::bytecode::BytecodeModule;
use crate::error::AtomicJsError;
use crate::value::Value;

use super::execution::execute_function;
use super::feedback::{FeedbackSink, TierOneState};
use super::state::{LocalSlot, VmPools};

pub(super) fn call_local0<F: FeedbackSink>(
    module: &BytecodeModule,
    local: &LocalSlot,
    feedback: &mut F,
    pools: &VmPools,
    tier_one: &mut TierOneState<'_>,
) -> Result<Value, AtomicJsError> {
    match local {
        LocalSlot::Plain(Value::Function(function_data)) => execute_function(
            module,
            function_data.function_index,
            &[],
            &function_data.captured_env,
            feedback,
            pools,
            tier_one,
        ),
        _ => match local.get() {
            Value::Function(function_data) => execute_function(
                module,
                function_data.function_index,
                &[],
                &function_data.captured_env,
                feedback,
                pools,
                tier_one,
            ),
            other => Err(AtomicJsError(format!(
                "attempted to call a non-function value: {other}"
            ))),
        },
    }
}

pub(super) fn call_value<F: FeedbackSink>(
    module: &BytecodeModule,
    stack: &mut Vec<Value>,
    argc: usize,
    feedback: &mut F,
    pools: &VmPools,
    tier_one: &mut TierOneState<'_>,
) -> Result<Value, AtomicJsError> {
    let callee = stack.pop().expect("Call needs a callee on the stack");
    let function_data = match callee {
        Value::Function(function) => function,
        other => {
            return Err(AtomicJsError(format!(
                "attempted to call a non-function value: {other}"
            )))
        }
    };
    if argc == 0 {
        return execute_function(
            module,
            function_data.function_index,
            &[],
            &function_data.captured_env,
            feedback,
            pools,
            tier_one,
        );
    }

    let mut args = pools.take_args();
    for _ in 0..argc {
        args.push(stack.pop().expect("Call needs argc values on the stack"));
    }
    args.reverse();
    let result = execute_function(
        module,
        function_data.function_index,
        &args,
        &function_data.captured_env,
        feedback,
        pools,
        tier_one,
    );
    pools.return_args(args);
    result
}

pub(super) fn call_direct<F: FeedbackSink>(
    module: &BytecodeModule,
    stack: &mut Vec<Value>,
    function_index: usize,
    argc: usize,
    feedback: &mut F,
    pools: &VmPools,
    tier_one: &mut TierOneState<'_>,
) -> Result<Value, AtomicJsError> {
    let mut args = pools.take_args();
    for _ in 0..argc {
        args.push(
            stack
                .pop()
                .expect("CallDirect needs argc values on the stack"),
        );
    }
    args.reverse();
    let result = execute_function(
        module,
        function_index,
        &args,
        &[],
        feedback,
        pools,
        tier_one,
    );
    pools.return_args(args);
    result
}
