//! Locale-aware rendering for structured compiler diagnostics.

#![forbid(unsafe_code)]

mod argument;
mod catalog;
mod locale;
mod rendered_diagnostic;
mod renderer;

pub use locale::DiagnosticLocale;
pub use rendered_diagnostic::{
    RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNote, RenderedDiagnosticNoteKind,
};
pub use renderer::DiagnosticRenderer;
