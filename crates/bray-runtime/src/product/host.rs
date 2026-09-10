mod attachment;
mod descriptor;
mod formation;
mod model;
mod operations;
mod thread;

pub(in crate::product) use model::initialize_thread_static_registry;
pub(super) use operations::{
    acquire_thread_attachment, discard_thread_attachment, mark_thread_attachment_acquired,
    observation_with_status, prepare_thread_attachment,
};
pub(crate) use operations::{control, register_thread_static, thread_attachment_identity};
pub(crate) use thread::drain_product_thread_statics;
