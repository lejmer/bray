use std::fmt::{self, Write};

use bray_source::{SourceSnapshot, TextRange, TextSize};

use crate::green::GreenNode;
use crate::text::text_from_writer;
use crate::{SyntaxKind, SyntaxToken};

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
    /// Returns the immutable source snapshot this node was parsed from.
    fn source(&self) -> &SourceSnapshot;

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

/// A syntax node whose immediate children are separated by punctuation.
pub trait SeparatedSyntaxNode: SyntaxNode {
    /// Returns this node's separator tokens in source order, including missing tokens retained
    /// by recovery. Separators inside nested nodes are excluded.
    fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_;
}

pub(crate) trait GreenSeparatedSyntaxNode: GreenSyntaxNode {
    const SEPARATOR_KIND: SyntaxKind = SyntaxKind::CommaToken;
}

impl<T: GreenSeparatedSyntaxNode> SeparatedSyntaxNode for T {
    fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.green_node()
            .child_tokens(self.start())
            .filter(|token| token.kind() == Self::SEPARATOR_KIND)
    }
}

pub(crate) trait GreenSyntaxNode: Send + Sync {
    fn green_node(&self) -> &GreenNode;

    fn start(&self) -> TextSize;

    fn range_description(&self) -> &'static str;

    fn is_recovered(&self) -> bool {
        false
    }
}

impl<T> SyntaxNode for T
where
    T: GreenSyntaxNode,
{
    fn kind(&self) -> SyntaxKind {
        self.green_node().kind()
    }

    fn full_range(&self) -> TextRange {
        full_range_from_width(
            self.start(),
            self.green_node().full_width(),
            self.range_description(),
        )
    }

    fn is_recovered(&self) -> bool {
        GreenSyntaxNode::is_recovered(self)
    }
}

pub(crate) trait GreenSourceSyntaxNode: GreenSyntaxNode {
    fn source(&self) -> &SourceSnapshot;
}

impl<T> SourceSyntaxNode for T
where
    T: GreenSourceSyntaxNode,
{
    fn source(&self) -> &SourceSnapshot {
        GreenSourceSyntaxNode::source(self)
    }

    fn write_full_text_from(&self, source_text: &str, writer: &mut dyn Write) -> fmt::Result {
        self.green_node()
            .write_source_text(source_text, self.start(), writer)
    }
}

pub(crate) fn full_range_from_width(
    start: TextSize,
    width: TextSize,
    node_description: &'static str,
) -> TextRange {
    match TextRange::with_len(start, width) {
        Some(range) => range,
        None => panic!("green {node_description} width must fit in TextRange"),
    }
}
