//! Source files, source identifiers, spans, text ranges, and source maps.

#![forbid(unsafe_code)]

mod id;
mod span;
mod text;

pub use id::SourceId;
pub use span::SourceSpan;
pub use text::{TextRange, TextSize, TextSizeOverflow};
