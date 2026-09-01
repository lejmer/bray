mod artifact;
mod dependency;
mod document;
mod emission;
mod external;
mod identity;
mod interface;
mod linking;
mod model;
mod project;
mod protocol;
mod runtime;
mod semantic;
mod source;
mod standard_library;
mod target;

pub use artifact::{
    DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm, DiagnosticArtifactKind,
    DiagnosticOutputSink, DiagnosticRuntimeArtifactProblem, DiagnosticRuntimeArtifactPurpose,
};
pub use dependency::{DiagnosticDependencyRequirementKind, DiagnosticDependencySubjectKind};
pub use document::DiagnosticDocumentParseKind;
pub use emission::{
    DiagnosticArtifactRequirement, DiagnosticAssemblySyntaxKind,
    DiagnosticCodegenVerificationStage, DiagnosticDebugInformationMode, DiagnosticDebugOutputMode,
    DiagnosticEmissionArtifactOperation, DiagnosticLoweringFailure, DiagnosticLoweringFailureKind,
    DiagnosticLoweringInputFailure, DiagnosticLoweringInputFailureKind,
    DiagnosticMirUnitBuildFailure, DiagnosticNativeLinkInputFailure,
    DiagnosticNativeProductFailureKind, DiagnosticProductKind, DiagnosticSourceConstructKind,
};
pub use external::{
    DiagnosticExternalToolFailureKind, DiagnosticExternalToolOperation, DiagnosticIoErrorKind,
};
pub use linking::{
    DiagnosticLinkInputKind, DiagnosticLinkOptimizationReportProblem, DiagnosticLinkRequirement,
    DiagnosticLinkRequirementKind, DiagnosticLinkedArtifactKind, DiagnosticLinkedProductKind,
};
pub use model::DiagnosticArg;
pub use protocol::{DiagnosticArgName, DiagnosticArgValue};
pub use runtime::DiagnosticRuntimeAbiVersion;
pub use semantic::{
    DiagnosticAlignmentKind, DiagnosticCallableAbi, DiagnosticNamedType, DiagnosticSelectionKind,
    DiagnosticType, DiagnosticTypeArgument,
};
pub use source::{DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticVisibility};
pub use standard_library::{
    DiagnosticStandardLibraryManifestProblem, DiagnosticStandardLibraryOptimizationMetadataProblem,
};
pub use target::DiagnosticTargetRepresentation;

#[cfg(test)]
mod tests;
