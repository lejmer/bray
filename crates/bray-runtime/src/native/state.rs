mod binding;
mod core;
mod execution;

pub(super) use binding::{current_native_task, runtime_failure, with_cleanup_runtime};
#[cfg(test)]
pub(crate) use core::test_runtime_isolation;
#[cfg(test)]
pub(super) use core::with_current_task;
pub(super) use core::{
    NativeRuntimeCore, StartedTask, initialize, run_worker, shutdown,
    with_independent_execution_context, with_root_execution_context, with_runtime,
};
pub(crate) use core::{RetainedRuntime, retain_current_runtime, retain_runtime};

#[cfg(feature = "test-output")]
pub(crate) use core::initialize_thread_context;
