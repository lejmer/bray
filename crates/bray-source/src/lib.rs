//! Source files, source identifiers, spans, text ranges, and source maps.

#![forbid(unsafe_code)]

mod id;
mod input;
mod line_index;
mod location;
mod origin;
mod snapshot;
mod span;
mod store;
mod text;

pub use id::SourceId;
pub use input::{SourceInput, SourceInputKind};
pub use line_index::{LineColumn, LineIndex, LspPosition};
pub use location::SourceLocation;
pub use origin::{SourceOrigin, SourceOriginKind};
pub use snapshot::{SourceChecksum, SourceRevision, SourceSnapshot};
pub use span::SourceSpan;
pub use store::{SourceStore, SourceStoreError};
pub use text::{TextRange, TextSize, TextSizeOverflow};
