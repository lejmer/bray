mod effects;
mod deferred;
mod slots;
mod stored;
mod targets;
mod transition;

mod projection;

mod source;
pub(in crate::analysis::storage_flow) use source::Source;

pub(super) use slots::{exposure_region, storage_subtree};
