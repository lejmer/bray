mod error;
mod plan;
mod host;
mod implementation;
mod link;

pub use error::NativeProductPlanningError;
pub(in crate::compilation) use error::codegen_preparation_failure_kind;
pub use plan::NativeProductPlan;
pub(in crate::compilation) use host::demanded_runtime_capabilities;
