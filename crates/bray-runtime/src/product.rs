mod attachment;
mod cleanup;
mod execution;
mod host;

#[cfg(test)]
pub(crate) use cleanup::empty_native_incident;
pub(crate) use host::{
    control, control_with_execution, drain_product_thread_statics, register_thread_static,
    retain_provider, thread_attachment_identity,
};

pub(crate) use execution::{
    ProductExecution, RetainedProductExecution, replace_execution_factory, retain_execution,
    with_execution_factory,
};
