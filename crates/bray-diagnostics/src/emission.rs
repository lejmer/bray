mod checker;
mod failure;
mod unsupported;

pub use checker::{
    DiagnosticCheckerFailure, DiagnosticCheckerLocal, DiagnosticCheckerNode,
    DiagnosticCheckerSymbol, DiagnosticConstantEvaluationFailure, DiagnosticConstantInputFailure,
    DiagnosticLiteralValueFailure, DiagnosticPatternInputFailure,
    DiagnosticSemanticSelectionFailure, DiagnosticSemanticSnapshotFailure,
    DiagnosticStorageFlowFailure,
};
pub use failure::{
    DiagnosticBindingFailure, DiagnosticEmissionArtifact, DiagnosticEmissionCodegenFailure,
    DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure,
    DiagnosticEmissionLinkPlanFailure, DiagnosticEmissionPlanningFailure,
    DiagnosticEmissionStagingFailure, DiagnosticPackageInterfaceFailure,
    DiagnosticSemanticQueryFailure, DiagnosticSemanticValueFailure,
};
pub use unsupported::{
    DiagnosticHostEnvironmentVariable, DiagnosticLlvmToolRole, DiagnosticUnsupportedEmissionReason,
};
