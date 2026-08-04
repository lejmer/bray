//! Locale-aware rendering for structured compiler diagnostics.

#![forbid(unsafe_code)]

mod argument;
mod build_progress;
mod catalog;
mod language_server;
mod locale;
mod rendered_diagnostic;
mod renderer;

pub use build_progress::{
    BuildProgressAction, BuildProgressConfiguration, BuildProgressField, BuildProgressLineKind,
    BuildProgressMessageRenderer, BuildProgressOperation,
};
pub use language_server::{LanguageServerMessage, LanguageServerMessageRenderer};
pub use locale::DiagnosticLocale;
pub use rendered_diagnostic::{
    RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNote, RenderedDiagnosticNoteKind,
};
pub use renderer::DiagnosticRenderer;
