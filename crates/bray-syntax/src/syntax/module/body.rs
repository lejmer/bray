use std::fmt;

use bray_source::{SourceSnapshot, TextRange, TextSize};

use crate::builder::{GreenNodeBuilder, RequiredSyntaxSlot, require_token_kind};
use crate::green::GreenNode;
use crate::node::{GreenSourceSyntaxNode, GreenSyntaxNode};
use crate::node_support::{contains_recovery, required_token, skipped_syntax};
use crate::{SkippedSyntax, SyntaxKind, SyntaxNode, SyntaxToken};

/// Body of a braced module declaration.
#[derive(Clone, Eq, Hash, PartialEq)]
pub struct ModuleBodySyntax {
    source: SourceSnapshot,
    node: GreenNode,
    start: TextSize,
}

impl ModuleBodySyntax {
    /// Creates a builder for a module-body node at `start`.
    pub fn builder(source: SourceSnapshot, start: TextSize) -> ModuleBodySyntaxBuilder {
        ModuleBodySyntaxBuilder::new(source, start)
    }

    pub(crate) fn from_green(source: SourceSnapshot, node: GreenNode, start: TextSize) -> Self {
        assert_eq!(node.kind(), SyntaxKind::ModuleBody);

        Self {
            source,
            node,
            start,
        }
    }

    pub(crate) fn into_green(self) -> GreenNode {
        self.node
    }

    fn from_builder(builder: ModuleBodySyntaxBuilder) -> Self {
        Self {
            source: builder.source.into_value(),
            node: builder.node.build(SyntaxKind::ModuleBody),
            start: builder.start,
        }
    }

    /// Returns the full source text range covered by this body.
    pub fn full_range(&self) -> TextRange {
        SyntaxNode::full_range(self)
    }

    /// Returns whether this body contains parser recovery.
    pub fn is_recovered(&self) -> bool {
        contains_recovery(&self.node, self.start)
    }

    /// Returns the required opening brace token.
    pub fn open_brace_token(&self) -> SyntaxToken {
        required_token(
            &self.node,
            self.start,
            SyntaxKind::OpenBraceToken,
            "module body",
        )
    }

    /// Returns the required closing brace token.
    pub fn close_brace_token(&self) -> SyntaxToken {
        required_token(
            &self.node,
            self.start,
            SyntaxKind::CloseBraceToken,
            "module body",
        )
    }

    /// Returns descendant skipped-syntax recovery nodes in source order.
    pub fn skipped_syntax(&self) -> impl Iterator<Item = SkippedSyntax> + '_ {
        skipped_syntax(&self.source, &self.node, self.start)
    }

    /// Returns descendant syntax tokens in source order.
    pub fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node.syntax_tokens(self.start)
    }
}

impl fmt::Debug for ModuleBodySyntax {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModuleBodySyntax")
            .field("source", &self.source)
            .field("kind", &SyntaxNode::kind(self))
            .field("full_range", &self.full_range())
            .field("is_recovered", &self.is_recovered())
            .finish()
    }
}

impl GreenSyntaxNode for ModuleBodySyntax {
    fn green_node(&self) -> &GreenNode {
        &self.node
    }

    fn start(&self) -> TextSize {
        self.start
    }

    fn range_description(&self) -> &'static str {
        "module-body"
    }

    fn is_recovered(&self) -> bool {
        ModuleBodySyntax::is_recovered(self)
    }
}

impl GreenSourceSyntaxNode for ModuleBodySyntax {
    fn source(&self) -> &SourceSnapshot {
        &self.source
    }
}

/// Builder for a module-body node.
pub struct ModuleBodySyntaxBuilder {
    source: RequiredSyntaxSlot<SourceSnapshot>,
    node: GreenNodeBuilder,
    start: TextSize,
}

impl ModuleBodySyntaxBuilder {
    /// Creates an empty module-body builder at `start`.
    pub fn new(source: SourceSnapshot, start: TextSize) -> Self {
        let mut source_slot = RequiredSyntaxSlot::new("module_body.source");

        source_slot.set(source);

        Self {
            source: source_slot,
            node: GreenNodeBuilder::new(),
            start,
        }
    }

    /// Appends the opening brace token.
    pub fn push_open_brace_token(&mut self, token: SyntaxToken) {
        require_token_kind(
            token.kind(),
            SyntaxKind::OpenBraceToken,
            "module_body.open_brace_token",
        );

        self.node.push_token(token);
    }

    /// Appends present source tokens under a skipped-syntax recovery node.
    pub fn push_skipped_tokens(&mut self, tokens: impl IntoIterator<Item = SyntaxToken>) {
        self.node.push_skipped_tokens(tokens);
    }

    /// Appends the closing brace token.
    pub fn push_close_brace_token(&mut self, token: SyntaxToken) {
        require_token_kind(
            token.kind(),
            SyntaxKind::CloseBraceToken,
            "module_body.close_brace_token",
        );

        self.node.push_token(token);
    }

    /// Builds the module-body node.
    pub fn build(self) -> ModuleBodySyntax {
        ModuleBodySyntax::from_builder(self)
    }
}

impl fmt::Debug for ModuleBodySyntaxBuilder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModuleBodySyntaxBuilder")
            .field("start", &self.start)
            .finish_non_exhaustive()
    }
}
