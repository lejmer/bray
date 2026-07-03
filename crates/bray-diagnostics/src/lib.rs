//! Structured diagnostics and diagnostic rendering data.

#![forbid(unsafe_code)]

mod argument;
mod diagnostic;
mod id;
mod kind;
mod label;
mod note;
mod severity;

pub use argument::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};
pub use diagnostic::Diagnostic;
pub use id::DiagnosticId;
pub use kind::DiagnosticKind;
pub use label::{DiagnosticLabel, DiagnosticLabelKind, DiagnosticLabelStyle};
pub use note::{DiagnosticNote, DiagnosticNoteKind};
pub use severity::SeverityKind;
