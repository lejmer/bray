//! Structured diagnostics and diagnostic rendering data.

#![forbid(unsafe_code)]

mod argument;
mod bag;
mod code;
mod diagnostic;
mod id;
mod interface;
mod kind;
mod label;
mod note;
mod result;
mod severity;

pub use argument::{
    DiagnosticAlignmentKind, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue,
    DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm, DiagnosticArtifactKind,
    DiagnosticCallableAbi, DiagnosticIoErrorKind, DiagnosticModuleTrust, DiagnosticNameKind,
    DiagnosticNamedType, DiagnosticOutputSink, DiagnosticRuntimeAbiVersion,
    DiagnosticSelectionKind, DiagnosticTargetRepresentation, DiagnosticType,
    DiagnosticTypeArgument, DiagnosticVisibility,
};
pub use bag::DiagnosticBag;
pub use code::DiagnosticCode;
pub use diagnostic::Diagnostic;
pub use id::DiagnosticId;
pub use interface::{DiagnosticInterfaceLimit, DiagnosticInterfaceSection};
pub use kind::DiagnosticKind;
pub use label::{DiagnosticLabel, DiagnosticLabelKind, DiagnosticLabelStyle};
pub use note::{DiagnosticNote, DiagnosticNoteKind};
pub use result::DiagnosticResult;
pub use severity::SeverityKind;
