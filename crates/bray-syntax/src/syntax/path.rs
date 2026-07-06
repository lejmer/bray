use std::fmt;

use bray_source::{SourceSnapshot, TextRange, TextSize};

use crate::builder::{GreenNodeBuilder, RequiredSyntaxSlot, require_token_kind};
use crate::green::GreenNode;
use crate::node::{GreenSourceSyntaxNode, GreenSyntaxNode, contains_recovery};
use crate::{SyntaxKind, SyntaxNode, SyntaxToken};

/// Dotted identifier path syntax node.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct PathSyntax {
    source: SourceSnapshot,
    node: GreenNode,
    start: TextSize,
}

impl PathSyntax {
    /// Creates a builder for a path node.
    pub fn builder(source: SourceSnapshot) -> PathSyntaxBuilder {
        PathSyntaxBuilder::new(source)
    }

    pub(crate) fn from_green(source: SourceSnapshot, node: GreenNode, start: TextSize) -> Self {
        assert_eq!(node.kind(), SyntaxKind::Path);

        Self {
            source,
            node,
            start,
        }
    }

    pub(crate) fn into_green(self) -> GreenNode {
        self.node
    }

    fn from_builder(builder: PathSyntaxBuilder) -> Self {
        Self {
            source: builder.source.into_value(),
            node: builder.node.build(SyntaxKind::Path),
            start: match builder.start {
                Some(start) => start,
                None => panic!("path must contain at least one token"),
            },
        }
    }

    /// Returns the full source text range covered by this path.
    pub fn full_range(&self) -> TextRange {
        SyntaxNode::full_range(self)
    }

    /// Returns whether this path contains parser recovery.
    pub fn is_recovered(&self) -> bool {
        contains_recovery(&self.node, self.start)
    }

    /// Returns identifier tokens in source order.
    pub fn identifier_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node
            .syntax_tokens(self.start)
            .filter(|token| token.kind() == SyntaxKind::IdentifierToken)
    }

    /// Returns dot tokens in source order.
    pub fn dot_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node
            .syntax_tokens(self.start)
            .filter(|token| token.kind() == SyntaxKind::DotToken)
    }

    /// Returns descendant syntax tokens in source order.
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node.syntax_tokens(self.start)
    }
}

impl fmt::Debug for PathSyntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PathSyntax")
            .field("source", &self.source)
            .field("kind", &SyntaxNode::kind(self))
            .field("full_range", &self.full_range())
            .field("is_recovered", &self.is_recovered())
            .finish()
    }
}

impl GreenSyntaxNode for PathSyntax {
    fn green_node(&self) -> &GreenNode {
        &self.node
    }

    fn start(&self) -> TextSize {
        self.start
    }

    fn range_description(&self) -> &'static str {
        "path"
    }

    fn is_recovered(&self) -> bool {
        PathSyntax::is_recovered(self)
    }
}

impl GreenSourceSyntaxNode for PathSyntax {
    fn source(&self) -> &SourceSnapshot {
        &self.source
    }
}

/// Builder for a path node.
pub struct PathSyntaxBuilder {
    source: RequiredSyntaxSlot<SourceSnapshot>,
    node: GreenNodeBuilder,
    start: Option<TextSize>,
}

impl PathSyntaxBuilder {
    /// Creates an empty path builder.
    pub fn new(source: SourceSnapshot) -> Self {
        let mut source_slot = RequiredSyntaxSlot::new("path.source");

        source_slot.set(source);

        Self {
            source: source_slot,
            node: GreenNodeBuilder::new(),
            start: None,
        }
    }

    /// Appends an identifier token in source order.
    pub fn push_identifier_token(&mut self, token: SyntaxToken) {
        require_token_kind(
            token.kind(),
            SyntaxKind::IdentifierToken,
            "path.identifier_token",
        );

        self.push_token(token);
    }

    /// Appends a dot token in source order.
    pub fn push_dot_token(&mut self, token: SyntaxToken) {
        require_token_kind(token.kind(), SyntaxKind::DotToken, "path.dot_token");

        self.push_token(token);
    }

    /// Builds the path node.
    pub fn build(self) -> PathSyntax {
        PathSyntax::from_builder(self)
    }

    fn push_token(&mut self, token: SyntaxToken) {
        self.set_start_if_empty(token.full_range().start());
        self.node.push_token(token);
    }

    fn set_start_if_empty(&mut self, start: TextSize) {
        if self.start.is_some() {
            return;
        }

        self.start = Some(start);
    }
}

impl fmt::Debug for PathSyntaxBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PathSyntaxBuilder")
            .finish_non_exhaustive()
    }
}
