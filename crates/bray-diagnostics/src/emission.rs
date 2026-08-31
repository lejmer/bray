mod checker;
mod failure;
mod unsupported;

pub use checker::{DiagnosticCheckerFailure, DiagnosticCheckerNode, DiagnosticCheckerSymbol};
pub use failure::{
    DiagnosticBindingFailure, DiagnosticEmissionArtifact, DiagnosticEmissionCodegenFailure,
    DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure,
    DiagnosticEmissionLinkPlanFailure, DiagnosticEmissionPlanningFailure,
    DiagnosticEmissionStagingFailure, DiagnosticPackageInterfaceFailure,
    DiagnosticSemanticValueFailure,
};
pub use unsupported::{
    DiagnosticHostEnvironmentVariable, DiagnosticLlvmToolRole, DiagnosticUnsupportedEmissionReason,
};
