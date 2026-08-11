//! Human-readable and machine-readable symbol tree inspection.

mod relationship;
mod report;

pub(crate) use report::{SymbolInspectionRenderError, render_symbol_inspection};
