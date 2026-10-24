//! Deterministic formatting for Bray source.

#![forbid(unsafe_code)]

mod configuration;
mod format;
mod operation;
mod rule;

pub use configuration::FormatterConfiguration;
pub use format::{FormattedSource, format_source_unit, format_text};
pub use operation::{
    FormatBytesError, FormatBytesErrorKind, FormatFileError, FormatFileErrorKind,
    FormatFileOutcome, FormatMode, format_bytes, format_file, format_files,
};
pub use rule::{
    FormatterDecision, FormatterDependency, FormatterParameter, FormatterParameterSchema,
    FormatterRule, FormatterRuleDescriptor, FormatterStage,
};
