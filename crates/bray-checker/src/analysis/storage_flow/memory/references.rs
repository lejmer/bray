mod deferred;
mod slots;
mod targets;
mod transition;

mod projection;

mod source;
pub(in crate::analysis::storage_flow) use source::Source;

pub(super) use deferred::{DeferredTransitions, merge_deferred};
