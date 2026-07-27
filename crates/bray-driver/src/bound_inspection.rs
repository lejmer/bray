//! Human-readable and machine-readable bound tree inspection.

mod locals;
mod report;
mod selection;
mod storage;

pub(crate) use report::render_bound_inspection;
