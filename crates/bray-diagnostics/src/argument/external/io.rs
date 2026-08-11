use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an I/O error-kind argument.
    pub const fn io_error_kind(kind: DiagnosticIoErrorKind) -> Self {
        Self::new(
            DiagnosticArgName::IoErrorKind,
            DiagnosticArgValue::IoErrorKind(kind),
        )
    }
}

/// Stable subset of host I/O error categories used in diagnostics.
///
/// This avoids storing localized or platform-specific error text in compiler
/// diagnostics while still preserving the relevant failure category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticIoErrorKind {
    /// A path or resource already exists.
    AlreadyExists,
    /// A path names a directory where a file was expected.
    IsDirectory,
    /// Input data was not accepted by the host API.
    InvalidData,
    /// An input argument was not accepted by the host API.
    InvalidInput,
    /// An operation was interrupted.
    Interrupted,
    /// A path component was not a directory.
    NotDirectory,
    /// A path or resource was not found.
    NotFound,
    /// Any I/O error category not yet modeled explicitly.
    Other,
    /// The host denied access to the resource.
    PermissionDenied,
    /// The operation timed out.
    TimedOut,
    /// The input ended unexpectedly.
    UnexpectedEof,
    /// The operation would have blocked.
    WouldBlock,
}

impl DiagnosticIoErrorKind {
    /// Returns the stable machine key for this I/O error category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyExists => "already_exists",
            Self::IsDirectory => "is_directory",
            Self::InvalidData => "invalid_data",
            Self::InvalidInput => "invalid_input",
            Self::Interrupted => "interrupted",
            Self::NotDirectory => "not_directory",
            Self::NotFound => "not_found",
            Self::Other => "other",
            Self::PermissionDenied => "permission_denied",
            Self::TimedOut => "timed_out",
            Self::UnexpectedEof => "unexpected_eof",
            Self::WouldBlock => "would_block",
        }
    }
}

impl From<std::io::ErrorKind> for DiagnosticIoErrorKind {
    fn from(kind: std::io::ErrorKind) -> Self {
        match kind {
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists,
            std::io::ErrorKind::IsADirectory => Self::IsDirectory,
            std::io::ErrorKind::InvalidData => Self::InvalidData,
            std::io::ErrorKind::InvalidInput => Self::InvalidInput,
            std::io::ErrorKind::Interrupted => Self::Interrupted,
            std::io::ErrorKind::NotADirectory => Self::NotDirectory,
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            std::io::ErrorKind::TimedOut => Self::TimedOut,
            std::io::ErrorKind::UnexpectedEof => Self::UnexpectedEof,
            std::io::ErrorKind::WouldBlock => Self::WouldBlock,
            _ => Self::Other,
        }
    }
}
