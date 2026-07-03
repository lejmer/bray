//! Locale-aware rendering for structured compiler diagnostics.

#![forbid(unsafe_code)]

mod argument;
mod catalog;
mod locale;
mod rendered;
mod renderer;

pub use locale::DiagnosticLocale;
pub use rendered::{RenderedDiagnostic, RenderedDiagnosticLabel, RenderedDiagnosticNote};
pub use renderer::DiagnosticRenderer;
