mod effects;
mod slots;
mod stored;
mod targets;
mod transition;

mod projection;
mod route;

mod source;
pub(in crate::analysis::storage_flow) use source::Source;
pub(in crate::analysis::storage_flow) use targets::borrow_chain_from;

pub(super) use slots::{exposure_region, storage_subtree, take_subtree};
