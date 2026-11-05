mod binding;
mod core;
mod execution;
mod owner;

#[cfg(test)]
pub(in crate::native) use core::with_task_services;

pub(super) use binding::runtime_failure;
pub(super) use owner::native_configuration;
pub(crate) use owner::with_cleanup_runtime;
#[cfg(test)]
pub(super) use core::current_task as current_native_task;
#[cfg(test)]
pub(crate) use core::test_runtime_isolation;
#[cfg(test)]
pub(super) use core::with_current_task;
pub(crate) use core::RuntimeRetirement;
pub(crate) use owner::{RetainedRuntime, retain_current_runtime, retain_runtime};
pub(super) use core::{
    bind_worker, current_reference_address, current_runtime, current_services, initialize,
    shutdown, unbind_worker, with_independent_execution_context, with_root_execution_context,
    with_runtime, worker_lanes,
};

#[cfg(feature = "test-output")]
pub(crate) use core::initialize_thread_context;

pub(crate) use core::NativeRuntimeCore;
