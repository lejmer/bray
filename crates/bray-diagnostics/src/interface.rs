mod context;
mod identity;
mod inventory;
mod native_artifact;
mod problem;
mod validation;

pub use context::{DiagnosticInterfaceSemanticRecordKind, DiagnosticInterfaceValidationContext};
pub use identity::{
    DiagnosticInterfaceDependency, DiagnosticInterfaceProductKind,
    DiagnosticPackageInterfaceIdentity,
};
pub use inventory::{DiagnosticInterfaceLimit, DiagnosticInterfaceSection};
pub use native_artifact::DiagnosticNativeArtifactCause;
pub use problem::{
    DiagnosticCheckedTemplateProblem, DiagnosticInterfaceDeclarationIdentity,
    DiagnosticInterfaceIdentitySurfaceProblem, DiagnosticInterfaceRelationship,
    DiagnosticInterfaceRelationshipKind, DiagnosticInterfaceSemanticProblem,
    DiagnosticInterfaceSymbolGraphProblem, DiagnosticInterfaceSymbolIdentity,
    DiagnosticInterfaceSymbolKind, DiagnosticInterfaceSymbolReference,
    DiagnosticInterfaceSynthesizedIdentity, DiagnosticSemanticContentProblem,
    DiagnosticSemanticValueKind,
};
pub use validation::{
    DiagnosticInterfaceCompressionFailure, DiagnosticInterfaceIntegerTarget,
    DiagnosticInterfaceMalformedCause, DiagnosticInterfaceUtf8Failure,
    DiagnosticInterfaceValidationFailure, DiagnosticInterfaceValidationField,
};
