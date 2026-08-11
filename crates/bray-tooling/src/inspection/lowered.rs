//! Lowered-unit inspection and MIR notation rendering.

mod model;
mod notation;
mod report;

pub(crate) use report::{
    LoweredInspectionRenderError, render_lowered_inspection, render_mir_inspection,
};
