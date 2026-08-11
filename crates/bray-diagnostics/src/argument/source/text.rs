use bray_source::TextSize;

use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates a source-name argument.
    pub fn source_name(name: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::SourceName,
            DiagnosticArgValue::SourceName(name.into()),
        )
    }

    /// Creates a source text-offset argument.
    pub const fn text_offset(offset: TextSize) -> Self {
        Self::new(
            DiagnosticArgName::TextOffset,
            DiagnosticArgValue::TextOffset(offset),
        )
    }

    /// Creates an exact source-token text argument.
    pub fn token_text(text: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::TokenText,
            DiagnosticArgValue::TokenText(text.into()),
        )
    }

    /// Creates a URI argument.
    pub fn uri(uri: impl Into<String>) -> Self {
        Self::new(DiagnosticArgName::Uri, DiagnosticArgValue::Uri(uri.into()))
    }

    /// Creates a worker-count argument.
    pub const fn worker_count(worker_count: u64) -> Self {
        Self::new(
            DiagnosticArgName::WorkerCount,
            DiagnosticArgValue::WorkerCount(worker_count),
        )
    }

    /// Creates complete source-input request context.
    pub const fn source_input(input: crate::DiagnosticSourceInput) -> Self {
        Self::new(
            DiagnosticArgName::SourceInput,
            DiagnosticArgValue::SourceInput(input),
        )
    }
}
