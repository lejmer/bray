mod candidate;
mod directive;
mod platform;

pub use candidate::{
    DiagnosticCallableArgumentRejection, DiagnosticConstructionInputRejection,
    DiagnosticReceiverCapability, DiagnosticRejectedSelectionCandidate,
    DiagnosticSelectionCandidate, DiagnosticSelectionCandidateIdentity,
    DiagnosticSelectionCandidateSignature, DiagnosticSelectionCandidates,
    DiagnosticSelectionCandidatesBuildError, DiagnosticSelectionRejectionReason,
    DiagnosticSelectionRejections,
};
pub use directive::{
    DiagnosticDirectiveArgumentProblem, DiagnosticNativeLinkDirectiveProblem,
    DiagnosticNativeLinkKind, DiagnosticNativeSymbolDirectiveProblem,
};
pub use platform::{
    DiagnosticPlatformAbiType, DiagnosticPlatformServiceRole,
    DiagnosticPlatformServiceSignatureProblem,
};
