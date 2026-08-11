use std::path::PathBuf;

use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an output-sink argument.
    pub const fn output_sink(sink: DiagnosticOutputSink) -> Self {
        Self::new(
            DiagnosticArgName::OutputSink,
            DiagnosticArgValue::OutputSink(sink),
        )
    }
}

/// Locale-neutral external output destination used by diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticOutputSink {
    /// Filesystem artifact path.
    Filesystem(PathBuf),
    /// Host-owned in-memory collector identity.
    Memory(String),
    /// Host-owned writable stream identity.
    Stream(String),
}
