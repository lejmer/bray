use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an external-tool failure-category argument.
    pub const fn external_tool_failure_kind(kind: DiagnosticExternalToolFailureKind) -> Self {
        Self::new(
            DiagnosticArgName::ExternalToolFailureKind,
            DiagnosticArgValue::ExternalToolFailureKind(kind),
        )
    }

    /// Creates an exact completed external-tool failure result argument.
    pub const fn external_tool_exit(exit: crate::DiagnosticExternalToolExit) -> Self {
        Self::new(
            DiagnosticArgName::ExternalToolExit,
            DiagnosticArgValue::ExternalToolExit(exit),
        )
    }
}

/// Locale-neutral external-tool failure categories beyond host I/O details.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticExternalToolFailureKind {
    Io,
    InvalidSize,
    ThreadIdentityExhausted,
    EventGenerationExhausted,
    InvalidEventIdentity,
    RuntimeThreadAlreadyInitialized,
    SynchronizationPoisoned,
    Unsupported,
    MissingOutputPipe,
    OutputCapture,
    OutputReaderTerminated,
}

impl DiagnosticExternalToolFailureKind {
    /// Returns the stable machine key for this failure category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::InvalidSize => "invalid_size",
            Self::ThreadIdentityExhausted => "thread_identity_exhausted",
            Self::EventGenerationExhausted => "event_generation_exhausted",
            Self::InvalidEventIdentity => "invalid_event_identity",
            Self::RuntimeThreadAlreadyInitialized => "runtime_thread_already_initialized",
            Self::SynchronizationPoisoned => "synchronization_poisoned",
            Self::Unsupported => "unsupported",
            Self::MissingOutputPipe => "missing_output_pipe",
            Self::OutputCapture => "output_capture",
            Self::OutputReaderTerminated => "output_reader_terminated",
        }
    }
}
