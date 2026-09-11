mod boundary;
mod cleanup;
mod registry;

pub use boundary::{
    bray_runtime_frame_storage_activation, bray_runtime_frame_storage_admission,
    bray_runtime_frame_storage_release,
};
pub(super) use cleanup::FrameShape;
#[cfg(test)]
pub(super) use registry::is_admitted;
pub(super) use registry::{FrameActivationClaim, claim};
