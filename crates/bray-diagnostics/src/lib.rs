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
    DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticArtifactDigest,
    DiagnosticArtifactDigestAlgorithm, DiagnosticArtifactKind, DiagnosticIoErrorKind,
    DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticOutputSink, DiagnosticVisibility,
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
