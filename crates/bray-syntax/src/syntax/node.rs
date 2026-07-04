use std::fmt::{self, Write};

use bray_source::TextRange;

use super::{SyntaxElements, text_from_writer};
use crate::SyntaxKind;

/// Common contract implemented by typed syntax tree nodes.
pub trait SyntaxNode: Send + Sync {
    /// Returns the stable kind for this syntax node.
    fn kind(&self) -> SyntaxKind;

    /// Returns the full source byte range covered by this node.
    fn full_range(&self) -> TextRange;
}

/// Common contract for syntax nodes contained in one source snapshot.
pub trait SourceSyntaxNode: SyntaxNode {
    /// Returns this node's ordered syntax elements.
    fn elements(&self) -> &SyntaxElements;

    /// Appends this node's exact source text using the owning source text.
    ///
    /// Panics when `source_text` does not contain the node's element ranges.
    fn write_full_text_from(&self, source_text: &str, writer: &mut dyn Write) -> fmt::Result {
        self.elements().write_source_text(source_text, writer)
    }

    /// Returns this node's exact source text using the owning source text.
    ///
    /// Panics when `source_text` does not contain the node's element ranges.
    fn full_text_from(&self, source_text: &str) -> String {
        text_from_writer(|writer| self.write_full_text_from(source_text, writer))
    }
}
