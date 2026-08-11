mod failure;
mod unsupported;

pub use failure::{
    DiagnosticEmissionArtifact, DiagnosticEmissionCodegenFailure,
    DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure,
    DiagnosticEmissionLinkPlanFailure, DiagnosticEmissionPlanningFailure,
    DiagnosticEmissionStagingFailure, DiagnosticPackageInterfaceFailure,
};
pub use unsupported::{
    DiagnosticHostEnvironmentVariable, DiagnosticLlvmToolRole, DiagnosticUnsupportedEmissionReason,
};
