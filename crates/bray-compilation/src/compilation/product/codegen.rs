mod error;
mod host;
mod implementation;
mod link;
mod plan;

pub use error::NativeProductPlanningError;
pub(in crate::compilation) use error::codegen_preparation_failure_kind;
pub(in crate::compilation) use host::demanded_runtime_capabilities;
pub use plan::NativeProductPlan;
