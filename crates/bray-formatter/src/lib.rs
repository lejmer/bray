//! Deterministic formatting for Bray source.

#![forbid(unsafe_code)]

mod configuration;
mod format;
mod operation;

pub use configuration::{FormatterConfiguration, FormatterRule};
pub use format::{FormattedSource, format_source_unit, format_text};
pub use operation::{
    FormatBytesError, FormatBytesErrorKind, FormatFileError, FormatFileErrorKind,
    FormatFileOutcome, FormatMode, format_bytes, format_file,
};
