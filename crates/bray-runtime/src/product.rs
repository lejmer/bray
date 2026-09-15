mod attachment;
mod cleanup;
mod host;

pub(crate) use cleanup::collect_finalizer_incidents;
pub(crate) use host::{
    control, drain_product_thread_statics, host_failure, register_thread_static,
    thread_attachment_identity,
};
