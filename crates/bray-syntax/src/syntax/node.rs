use std::fmt::{self, Write};

use bray_source::TextRange;

use crate::SyntaxKind;
use crate::text::text_from_writer;

/// Common contract implemented by typed syntax tree nodes.
pub trait SyntaxNode: Send + Sync {
    /// Returns the stable kind for this syntax node.
    fn kind(&self) -> SyntaxKind;

    /// Returns the full source byte range covered by this node.
    fn full_range(&self) -> TextRange;

    /// Returns whether this node is explicit syntax recovery.
    fn is_recovered(&self) -> bool {
        false
    }
}

/// Common contract for syntax nodes contained in one source snapshot.
pub trait SourceSyntaxNode: SyntaxNode {
    /// Appends this node's exact source text using the owning source text.
    ///
    /// Panics when `source_text` does not contain the node's element ranges.
    fn write_full_text_from(&self, source_text: &str, writer: &mut dyn Write) -> fmt::Result;

    /// Returns this node's exact source text using the owning source text.
    ///
    /// Panics when `source_text` does not contain the node's element ranges.
    fn full_text_from(&self, source_text: &str) -> String {
        text_from_writer(|writer| self.write_full_text_from(source_text, writer))
    }
}
