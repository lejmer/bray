mod attachment;
mod cleanup;
mod execution;
mod host;

pub(crate) use cleanup::{
    StaticCleanup, run_static_destroy, run_static_transition, run_synchronous_finalizer,
};

#[cfg(test)]
pub(crate) use cleanup::empty_native_incident;
pub(crate) use host::{
    ProductEntry, cleanup_capacity_admission, cleanup_capacity_discharge, control,
    control_with_execution, drain_product_thread_statics, register_thread_static, retain_provider,
    thread_attachment_identity,
};

pub(crate) use execution::{
    ProductCleanup, ProductExecution, RetainedProductExecution, replace_execution_factory,
    retain_execution, with_execution_factory,
};
