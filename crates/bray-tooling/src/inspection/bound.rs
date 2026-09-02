//! Human-readable and machine-readable bound tree inspection.

mod locals;
mod report;
mod selection;
mod storage;

pub(crate) use report::{BoundInspectionRenderError, render_bound_inspection};
pub(crate) use selection::SelectionInspectionError;
