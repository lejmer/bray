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
    DiagnosticOutputSink,
};
pub use dependency::{DiagnosticDependencyRequirementKind, DiagnosticDependencySubjectKind};
pub use document::DiagnosticDocumentParseKind;
pub use emission::{
    DiagnosticArtifactRequirement, DiagnosticAssemblySyntaxKind, DiagnosticDebugInformationMode,
    DiagnosticDebugOutputMode, DiagnosticEmissionArtifactOperation,
    DiagnosticNativeProductFailureKind, DiagnosticProductKind,
};
pub use external::{
    DiagnosticExternalToolFailureKind, DiagnosticExternalToolOperation, DiagnosticIoErrorKind,
};
pub use linking::{
    DiagnosticLinkInputKind, DiagnosticLinkRequirement, DiagnosticLinkRequirementKind,
    DiagnosticLinkedArtifactKind, DiagnosticLinkedProductKind,
};
pub use model::DiagnosticArg;
pub use protocol::{DiagnosticArgName, DiagnosticArgValue};
pub use runtime::DiagnosticRuntimeAbiVersion;
pub use semantic::{
    DiagnosticAlignmentKind, DiagnosticCallableAbi, DiagnosticNamedType, DiagnosticSelectionKind,
    DiagnosticType, DiagnosticTypeArgument,
};
pub use source::{DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticVisibility};
pub use standard_library::DiagnosticStandardLibraryManifestProblem;
pub use target::DiagnosticTargetRepresentation;

#[cfg(test)]
mod tests;
