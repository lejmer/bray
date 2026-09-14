mod calls;
mod core;
mod sources;

pub use core::infer_result_dependencies;
pub(super) use sources::normalized_subject;
