//! Locale-aware rendering for structured compiler diagnostics.

#![forbid(unsafe_code)]

mod argument;
mod build_progress;
mod catalog;
pub mod command_help;
mod language_server;
mod locale;
mod profile;
mod rendered_diagnostic;
mod renderer;
mod test_report;

pub use build_progress::{
    BuildProgressAction, BuildProgressConfiguration, BuildProgressLineKind,
    BuildProgressMessageRenderer, BuildProgressOperation, ProgressField,
};
pub use language_server::{LanguageServerMessage, LanguageServerMessageRenderer};
pub use locale::DiagnosticLocale;
pub use profile::CompilerProfileMessageRenderer;
pub use rendered_diagnostic::{
    RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNote,
    RenderedDiagnosticNoteKind, RenderedDiagnosticRelatedLocation, RenderedDiagnosticSuggestion,
};
pub use renderer::DiagnosticRenderer;
pub use test_report::{
    TestReportActivity, TestReportLineKind, TestReportMessageRenderer, TestReportOutcome,
    TestReportSummaryStatus,
};
