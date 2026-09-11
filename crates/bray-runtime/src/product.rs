mod attachment;
mod cleanup;
mod host;

#[cfg(test)]
pub(crate) use cleanup::empty_native_incident;
pub(crate) use host::{
    control, drain_product_thread_statics, register_thread_static, retain_provider,
    thread_attachment_identity,
};
