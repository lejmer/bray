mod attachment;
mod cleanup;
mod execution;
mod host;

pub(crate) use attachment::initialize_thread_attachment_storage;
pub(crate) use cleanup::collect_finalizer_incidents;
pub(crate) use execution::acquire as acquire_execution;
pub(crate) use host::{
    drain_product_thread_statics, initialize_thread_static_registry, register_thread_static,
    thread_attachment_identity,
};
