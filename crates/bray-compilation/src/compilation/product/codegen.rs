mod error;
mod host;
mod implementation;
mod link;
mod plan;
mod preparation;
mod reachability;
mod roots;

pub use error::NativeProductPlanningError;
pub(in crate::compilation) use error::{
    codegen_preparation_failure_kind, native_product_preparation_diagnostic,
};
pub use plan::NativeProductPlan;
