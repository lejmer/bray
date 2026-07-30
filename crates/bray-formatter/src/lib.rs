//! Deterministic formatting for Bray source.

#![forbid(unsafe_code)]

mod format;
mod operation;

pub use format::{FormattedSource, format_source_unit, format_text};
pub use operation::{
    FormatBytesError, FormatBytesErrorKind, FormatFileError, FormatFileErrorKind,
    FormatFileOutcome, FormatMode, format_bytes, format_file,
};
