mod codegen;
mod dependency;
mod entry;
mod lifecycle;
mod query;
mod realization;
mod specialization;
mod specialization_identity;
mod visibility;

pub(in crate::compilation) use codegen::codegen_fact_failure_kind;
pub(in crate::compilation) use codegen::demanded_runtime_capabilities;
pub use codegen::{NativeProductFactError, NativeProductFacts};
pub(in crate::compilation) use lifecycle::CodegenLifecycleNeeds;
#[cfg(test)]
pub(in crate::compilation) use realization::{generated_frame_symbol_name, generated_symbol_name};
pub(in crate::compilation) use specialization_identity::encoding::structural_type_identity;
