mod codegen;
mod dependency;
mod entry;
mod error;
mod lifecycle;
mod query;
mod realization;
mod specialization;
mod specialization_identity;
mod visibility;

pub use codegen::{NativeProductPlan, NativeProductPlanningError};
pub(in crate::compilation) use codegen::{
    codegen_preparation_failure_kind, mir_content_identity, native_product_preparation_diagnostic,
};
pub(crate) use codegen::{NativeDemand, NativeDemandReason};
pub(crate) use error::{
    ProductDataKind, ProductQueryContext, ProductQueryFailure, ProductSynchronizationComponent,
    ProductTestCatalogFailureKind, ProductValueKind,
};
pub use error::{ProductQueryError, ProductQueryErrorKind};
pub(in crate::compilation) use lifecycle::CodegenLifecycleNeeds;
#[cfg(test)]
pub(in crate::compilation) use realization::{generated_frame_symbol_name, generated_symbol_name};
pub(in crate::compilation) use specialization_identity::encoding::structural_type_identity;
