//! Compiled package-interface publication.

mod build;
mod error;
mod semantic;
mod template;

pub(in crate::compilation) use build::external_symbol_key;
pub use error::{
    PackageInterfaceExportContract, PackageInterfaceExportError,
    PackageInterfaceInvalidCompilationCause,
};
pub(in crate::compilation::export) use error::{
    binding_query_export_error, callable_signature_export_error,
    capacity_export_error, export_contract_error,
    checker_infrastructure_export_error, constant_callable_evaluation_export_error,
    executable_template_evaluation_export_error, fact_query_export_error,
    fragment_coordination_export_error, invalid_compilation_binding_error,
    invalid_compilation_fact_error, semantic_value_export_error,
};
