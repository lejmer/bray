//! Source files, source identifiers, spans, text ranges, and source maps.

#![forbid(unsafe_code)]

mod id;
mod identity;
mod input;
mod line_index;
mod loader;
mod location;
mod origin;
mod snapshot;
mod span;
mod store;
mod text;
mod version;

pub use id::SourceId;
pub use identity::SourceIdentity;
pub use input::{SourceInput, SourceInputKind};
pub use line_index::{LineColumn, LineIndex, LspPosition};
pub use loader::{SourceLoadError, SourceLoader};
pub use location::SourceLocation;
pub use origin::{SourceOrigin, SourceOriginKind};
pub use snapshot::{SourceChecksum, SourceSnapshot};
pub use span::SourceSpan;
pub use store::SourceStore;
pub use text::{TextRange, TextSize, TextSizeOverflow};
pub use version::SourceVersion;
