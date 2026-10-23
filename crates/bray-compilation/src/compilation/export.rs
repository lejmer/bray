//! Compiled package-interface publication.

mod build;
mod error;
mod semantic;
mod template;

pub(in crate::compilation) use build::external_symbol_key;
pub(in crate::compilation::export) use error::{
    PackageInterfaceExportContract, binding_query_export_error, callable_signature_export_error,
    capacity_export_error, constant_callable_evaluation_export_error,
    executable_template_evaluation_export_error, export_contract_error, fact_query_export_error,
    fragment_coordination_export_error,
};
pub use error::{PackageInterfaceExportError, PackageInterfaceInvalidCompilationCause};
