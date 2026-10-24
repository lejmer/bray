use std::io;
use std::path::PathBuf;

use bray_platform::PlatformError;

/// External-tool stream whose capture failed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExternalToolStream {
    /// Standard output.
    StandardOutput,
    /// Standard error.
    StandardError,
}

/// Response-file operation that failed.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExternalToolResponseFileOperation {
    /// Create and write the response file.
    Write,
    /// Remove the response file after invocation.
    Remove,
}

/// Structured failure from the compiler-host external-tool boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalToolFailure {
    /// Cancellation was observed before a complete result was available.
    Cancelled,
    /// A response-file operation failed.
    ResponseFile {
        /// Exact response-file path.
        path: PathBuf,
        /// Failed response-file operation.
        operation: ExternalToolResponseFileOperation,
        /// Stable host I/O failure category.
        kind: io::ErrorKind,
    },
    /// A native child-process operation failed.
    Process(PlatformError),
    /// Reading one captured output stream failed.
    OutputCapture {
        /// Stream whose capture failed.
        stream: ExternalToolStream,
        /// Stable host I/O failure category.
        kind: io::ErrorKind,
    },
}
