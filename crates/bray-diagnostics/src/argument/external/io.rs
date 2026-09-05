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
    /// Storage is full.
    StorageFull,
    /// Storage quota exceeded.
    QuotaExceeded,
    /// File exceeds the supported size.
    FileTooLarge,
    /// Filesystem is read-only.
    ReadOnlyFilesystem,
    /// Resource is busy.
    ResourceBusy,
    /// Executable file is in use.
    ExecutableFileBusy,
    /// Operation crosses filesystem devices.
    CrossesDevices,
    /// File has too many hard links.
    TooManyLinks,
    /// Filename violates filesystem requirements.
    InvalidFilename,
    /// The write made no progress.
    WriteZero,
    /// Host memory is exhausted.
    OutOfMemory,
    /// Resource does not support seeking.
    NotSeekable,
    /// Directory is not empty.
    DirectoryNotEmpty,
    /// Filesystem does not support the operation.
    Unsupported,
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
            Self::StorageFull => "storage_full",
            Self::QuotaExceeded => "quota_exceeded",
            Self::FileTooLarge => "file_too_large",
            Self::ReadOnlyFilesystem => "read_only_filesystem",
            Self::ResourceBusy => "resource_busy",
            Self::ExecutableFileBusy => "executable_file_busy",
            Self::CrossesDevices => "crosses_devices",
            Self::TooManyLinks => "too_many_links",
            Self::InvalidFilename => "invalid_filename",
            Self::WriteZero => "write_zero",
            Self::OutOfMemory => "out_of_memory",
            Self::NotSeekable => "not_seekable",
            Self::DirectoryNotEmpty => "directory_not_empty",
            Self::Unsupported => "unsupported",
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
            std::io::ErrorKind::StorageFull => Self::StorageFull,
            std::io::ErrorKind::QuotaExceeded => Self::QuotaExceeded,
            std::io::ErrorKind::FileTooLarge => Self::FileTooLarge,
            std::io::ErrorKind::ReadOnlyFilesystem => Self::ReadOnlyFilesystem,
            std::io::ErrorKind::ResourceBusy => Self::ResourceBusy,
            std::io::ErrorKind::ExecutableFileBusy => Self::ExecutableFileBusy,
            std::io::ErrorKind::CrossesDevices => Self::CrossesDevices,
            std::io::ErrorKind::TooManyLinks => Self::TooManyLinks,
            std::io::ErrorKind::InvalidFilename => Self::InvalidFilename,
            std::io::ErrorKind::WriteZero => Self::WriteZero,
            std::io::ErrorKind::OutOfMemory => Self::OutOfMemory,
            std::io::ErrorKind::NotSeekable => Self::NotSeekable,
            std::io::ErrorKind::DirectoryNotEmpty => Self::DirectoryNotEmpty,
            std::io::ErrorKind::Unsupported => Self::Unsupported,
            _ => Self::Other,
        }
    }
}
