use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bray_source::TextSizeOverflow;

use crate::{FormattedSource, format_text};

const UTF8_BYTE_ORDER_MARK: &[u8] = b"\xef\xbb\xbf";

/// Stable category for a source-byte formatting failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FormatBytesErrorKind {
    /// Source bytes were not valid UTF-8.
    InvalidUtf8,
    /// Source text exceeded the formatter's compact source range.
    SourceTooLarge,
}

/// Typed source-byte formatting failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FormatBytesError {
    kind: FormatBytesErrorKind,
    byte_count: usize,
    invalid_utf8_at: Option<usize>,
}

impl FormatBytesError {
    /// Returns the stable failure category.
    pub const fn kind(self) -> FormatBytesErrorKind {
        self.kind
    }

    /// Returns the source byte count after an accepted leading byte order mark.
    pub const fn byte_count(self) -> usize {
        self.byte_count
    }

    /// Returns the first invalid UTF-8 byte offset when available.
    pub const fn invalid_utf8_at(self) -> Option<usize> {
        self.invalid_utf8_at
    }

    fn invalid_utf8(byte_count: usize, valid_up_to: usize) -> Self {
        Self {
            kind: FormatBytesErrorKind::InvalidUtf8,
            byte_count,
            invalid_utf8_at: Some(valid_up_to),
        }
    }

    fn source_too_large(byte_count: usize, _error: TextSizeOverflow) -> Self {
        Self {
            kind: FormatBytesErrorKind::SourceTooLarge,
            byte_count,
            invalid_utf8_at: None,
        }
    }
}

/// Filesystem behavior selected for a formatting operation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FormatMode {
    /// Report whether formatting is needed without changing the file.
    Check,
    /// Write changed formatted source back to the file.
    Write,
}

/// Result of formatting one source file.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FormatFileOutcome {
    /// The file already matched the formatter output.
    Unchanged,
    /// The file needs formatting and check mode left it unchanged.
    WouldChange,
    /// The formatter wrote changed source to the file.
    Written,
}

impl FormatFileOutcome {
    /// Returns whether the input file differed from formatted output.
    pub const fn changed(self) -> bool {
        matches!(self, Self::WouldChange | Self::Written)
    }
}

/// Stable category for a source-file formatting failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FormatFileErrorKind {
    /// Source bytes could not be read.
    Read,
    /// Source bytes were not valid UTF-8.
    InvalidUtf8,
    /// Source text exceeded the formatter's compact source range.
    SourceTooLarge,
    /// Formatted source bytes could not be written.
    Write,
}

/// Typed source-file formatting failure.
#[derive(Debug)]
pub struct FormatFileError {
    kind: FormatFileErrorKind,
    path: PathBuf,
    io_error_kind: Option<io::ErrorKind>,
    invalid_utf8_at: Option<usize>,
}

impl FormatFileError {
    /// Returns the stable failure category.
    pub const fn kind(&self) -> FormatFileErrorKind {
        self.kind
    }

    /// Returns the file path associated with the failure.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the underlying I/O error category when the failure came from I/O.
    pub const fn io_error_kind(&self) -> Option<io::ErrorKind> {
        self.io_error_kind
    }

    /// Returns the first invalid UTF-8 byte offset when available.
    pub const fn invalid_utf8_at(&self) -> Option<usize> {
        self.invalid_utf8_at
    }

    fn io(kind: FormatFileErrorKind, path: &Path, error: &io::Error) -> Self {
        Self {
            kind,
            path: path.to_path_buf(),
            io_error_kind: Some(error.kind()),
            invalid_utf8_at: None,
        }
    }

    fn invalid_utf8(path: &Path, valid_up_to: usize) -> Self {
        Self {
            kind: FormatFileErrorKind::InvalidUtf8,
            path: path.to_path_buf(),
            io_error_kind: None,
            invalid_utf8_at: Some(valid_up_to),
        }
    }

    fn source_too_large(path: &Path) -> Self {
        Self {
            kind: FormatFileErrorKind::SourceTooLarge,
            path: path.to_path_buf(),
            io_error_kind: None,
            invalid_utf8_at: None,
        }
    }
}

/// Formats one raw Bray source input.
///
/// Source bytes must be UTF-8. An accepted leading UTF-8 byte order mark is
/// retained in the formatted output and excluded from reported byte offsets.
pub fn format_bytes(bytes: &[u8]) -> Result<FormattedSource, FormatBytesError> {
    let (has_byte_order_mark, source_bytes) = match bytes.strip_prefix(UTF8_BYTE_ORDER_MARK) {
        Some(source_bytes) => (true, source_bytes),
        None => (false, bytes),
    };

    let source_text = std::str::from_utf8(source_bytes).map_err(|error| {
        FormatBytesError::invalid_utf8(source_bytes.len(), error.valid_up_to())
    })?;

    let formatted = format_text(source_text)
        .map_err(|error| FormatBytesError::source_too_large(source_bytes.len(), error))?;

    Ok(if has_byte_order_mark {
        formatted.with_leading_byte_order_mark()
    } else {
        formatted
    })
}

/// Formats one UTF-8 Bray source file in check or write mode.
///
/// A leading UTF-8 byte order mark is retained when write mode changes the
/// file. Unchanged files are never rewritten.
pub fn format_file(
    path: impl AsRef<Path>,
    mode: FormatMode,
) -> Result<FormatFileOutcome, FormatFileError> {
    let path = path.as_ref();

    let bytes = fs::read(path)
        .map_err(|error| FormatFileError::io(FormatFileErrorKind::Read, path, &error))?;

    let formatted = format_bytes(&bytes).map_err(|error| match error.kind() {
        FormatBytesErrorKind::InvalidUtf8 => {
            let valid_up_to = error
                .invalid_utf8_at()
                .unwrap_or_else(|| unreachable!("invalid UTF-8 must carry its byte offset"));

            FormatFileError::invalid_utf8(path, valid_up_to)
        }
        FormatBytesErrorKind::SourceTooLarge => {
            FormatFileError::source_too_large(path)
        }
    })?;

    if !formatted.changed() {
        return Ok(FormatFileOutcome::Unchanged);
    }

    if mode == FormatMode::Check {
        return Ok(FormatFileOutcome::WouldChange);
    }

    fs::write(path, formatted.text().as_bytes())
        .map_err(|error| FormatFileError::io(FormatFileErrorKind::Write, path, &error))?;

    Ok(FormatFileOutcome::Written)
}
