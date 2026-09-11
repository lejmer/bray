mod attachment;
mod cleanup;
mod descriptor;
mod entry;
mod formation;
mod model;
mod operations;
mod retention;
mod thread;

pub(crate) use entry::ProductEntry;
pub(super) use model::host_status;
pub(in crate::product) use model::initialize_thread_static_registry;
pub(crate) use operations::{
    control, control_with_execution, register_thread_static, thread_attachment_identity,
};
pub(super) use operations::{observation_with_status, prepare_thread_attachment};
pub(crate) use retention::retain_provider;
pub(crate) use thread::drain_product_thread_statics;
