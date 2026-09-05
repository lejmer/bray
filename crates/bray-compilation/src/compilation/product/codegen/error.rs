mod backend;
mod context;
mod link_input;
mod model;
mod preparation;
mod presentation;
mod query;
mod runtime_selection;
mod synthetic;

#[cfg(test)]
mod tests;

pub(in crate::compilation::product::codegen) use model::native_batch_error;
pub use model::{NativeLinkInputPlanningError, NativeProductPlanningError};
pub(in crate::compilation) use preparation::codegen_preparation_failure_kind;
pub(in crate::compilation) use presentation::native_product_preparation_diagnostic;
