use std::fmt;

use bray_source::{SourceSnapshot, TextRange, TextSize};

use crate::green::GreenNode;
use crate::node::{GreenSourceSyntaxNode, GreenSyntaxNode};
use crate::{SyntaxKind, SyntaxToken};

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

    /// Returns whether this node is explicit syntax recovery.
    pub const fn is_recovered(&self) -> bool {
        true
    }

    /// Returns the full source text range covered by skipped tokens.
    pub fn full_range(&self) -> TextRange {
        crate::SyntaxNode::full_range(self)
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
            .field("kind", &crate::SyntaxNode::kind(self))
            .field("full_range", &self.full_range())
            .finish()
    }
}

impl GreenSyntaxNode for SkippedSyntax {
    fn green_node(&self) -> &GreenNode {
        &self.node
    }

    fn start(&self) -> TextSize {
        self.start
    }

    fn range_description(&self) -> &'static str {
        "skipped-syntax"
    }

    fn is_recovered(&self) -> bool {
        SkippedSyntax::is_recovered(self)
    }
}

impl GreenSourceSyntaxNode for SkippedSyntax {
    fn source(&self) -> &SourceSnapshot {
        &self.source
    }
}

pub(crate) fn skipped_syntax_nodes<'syntax>(
    source: &'syntax SourceSnapshot,
    node: &'syntax GreenNode,
    start: TextSize,
) -> impl Iterator<Item = SkippedSyntax> + 'syntax {
    node.skipped_syntax_nodes(start).map(|(node, start)| {
        // SourceSnapshot clones share immutable source text with typed recovery nodes.
        SkippedSyntax::from_green(source.clone(), node, start)
    })
}
