mod attachment;
mod cleanup;
mod host;
mod incident;

pub(crate) use cleanup::{empty_native_incident, incidents_from_status};
pub(crate) use host::{
    control, drain_product_thread_statics, register_thread_static, thread_attachment_identity,
};
pub(crate) use incident::CleanupIncident;
