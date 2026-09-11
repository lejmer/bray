mod boundary;
mod cleanup;
mod registry;

pub use boundary::{
    bray_runtime_cleanup_capacity_admission, bray_runtime_cleanup_capacity_discharge,
    bray_runtime_frame_storage_activation, bray_runtime_frame_storage_admission,
    bray_runtime_frame_storage_release,
};
pub(super) use cleanup::FrameShape;
pub(super) use registry::{FrameActivationClaim, claim};
#[cfg(test)]
pub(super) use registry::is_admitted;
