mod body;
mod diagnostic;
mod model;
mod query;

pub use diagnostic::diagnostic_native_artifact_cause;

pub(in crate::compilation) use body::LoadedImplementation;
pub(in crate::compilation) use diagnostic::{
    implementation_validation_diagnostics, native_artifact_diagnostics,
    standard_library_diagnostics,
};
pub(super) use model::LoadedDependencyInterface;
