mod attachment;
mod cleanup;
mod host;

pub(crate) use cleanup::{empty_native_incident, finish_finalizer_callback};
pub(crate) use host::{
    control, drain_product_thread_statics, register_thread_static, thread_attachment_identity,
};
