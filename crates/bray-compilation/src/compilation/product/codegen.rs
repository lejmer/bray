mod error;
mod host;
mod implementation;
mod link;
mod plan;
mod reachability;
mod roots;

pub use error::NativeProductPlanningError;
pub(in crate::compilation) use error::{
    codegen_preparation_failure_kind, native_product_preparation_diagnostic,
};
pub(in crate::compilation) use host::demanded_runtime_capabilities;
pub use plan::NativeProductPlan;
