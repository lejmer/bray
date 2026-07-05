use std::fmt::{self, Write};

use bray_source::{SourceSnapshot, TextRange, TextSize};

use super::{SourceSyntaxNode, SyntaxNode};
use crate::green::GreenNode;
use crate::{SyntaxKind, SyntaxText, SyntaxToken};

/// Recovery node containing present tokens skipped by the parser.
///
/// Skipped syntax preserves exact source text at a recovery point while making
/// the recovered region visible to later syntax consumers.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct SkippedSyntax {
    source: SourceSnapshot,
    node: GreenNode,
    start: TextSize,
}

impl SkippedSyntax {
    pub(crate) fn from_green(source: SourceSnapshot, node: GreenNode, start: TextSize) -> Self {
        assert_eq!(node.kind(), SyntaxKind::SkippedSyntax);

        Self {
            source,
            node,
            start,
        }
    }

    /// Returns this node's stable syntax kind.
    pub fn kind(&self) -> SyntaxKind {
        self.node.kind()
    }

    /// Returns whether this node is explicit syntax recovery.
    pub const fn is_recovered(&self) -> bool {
        true
    }

    /// Returns the full source text range covered by skipped tokens.
    pub fn full_range(&self) -> TextRange {
        match TextRange::with_len(self.start, self.node.full_width()) {
            Some(range) => range,
            None => panic!("green skipped-syntax width must fit in TextRange"),
        }
    }

    /// Returns the immutable source snapshot this recovery node was parsed from.
    pub const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    /// Returns the skipped syntax tokens in source order.
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node.syntax_tokens(self.start)
    }
}

impl fmt::Debug for SkippedSyntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SkippedSyntax")
            .field("source", &self.source)
            .field("kind", &self.kind())
            .field("full_range", &self.full_range())
            .finish()
    }
}

impl SyntaxNode for SkippedSyntax {
    fn kind(&self) -> SyntaxKind {
        SkippedSyntax::kind(self)
    }

    fn full_range(&self) -> TextRange {
        SkippedSyntax::full_range(self)
    }

    fn is_recovered(&self) -> bool {
        SkippedSyntax::is_recovered(self)
    }
}

impl SourceSyntaxNode for SkippedSyntax {
    fn write_full_text_from(&self, source_text: &str, writer: &mut dyn Write) -> fmt::Result {
        self.node.write_source_text(source_text, self.start, writer)
    }
}

impl SyntaxText for SkippedSyntax {
    fn write_full_text(&self, writer: &mut dyn Write) -> fmt::Result {
        self.write_full_text_from(self.source.text(), writer)
    }
}
