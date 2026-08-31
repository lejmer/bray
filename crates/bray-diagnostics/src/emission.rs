mod failure;
mod unsupported;

pub use failure::{
    DiagnosticBindingFailure, DiagnosticCheckerFailure, DiagnosticEmissionArtifact,
    DiagnosticEmissionCodegenFailure, DiagnosticEmissionEvaluationFailure,
    DiagnosticEmissionFailure, DiagnosticEmissionLinkPlanFailure,
    DiagnosticEmissionPlanningFailure, DiagnosticEmissionStagingFailure,
    DiagnosticPackageInterfaceFailure, DiagnosticSemanticValueFailure,
};
pub use unsupported::{
    DiagnosticHostEnvironmentVariable, DiagnosticLlvmToolRole, DiagnosticUnsupportedEmissionReason,
};
