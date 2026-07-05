use bray_source::{SourceSnapshot, TextSize};

use crate::builder::{GreenNodeBuilder, require_token_kind};
use crate::green::GreenNode;
use crate::syntax::recovery::SkippedSyntax;
use crate::{SyntaxKind, SyntaxToken};

#[derive(Clone, Eq, Hash, PartialEq)]
pub(in crate::syntax) struct SeparatedSyntaxList {
    source: SourceSnapshot,
    node: GreenNode,
    start: TextSize,
}

impl SeparatedSyntaxList {
    pub(in crate::syntax) fn from_green(
        source: SourceSnapshot,
        node: GreenNode,
        start: TextSize,
        expected_kind: SyntaxKind,
    ) -> Self {
        assert_eq!(node.kind(), expected_kind);

        Self {
            source,
            node,
            start,
        }
    }

    pub(in crate::syntax) fn is_recovered(&self) -> bool {
        self.tokens().any(|token| token.is_missing()) || self.skipped_syntax().next().is_some()
    }

    pub(in crate::syntax) const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    pub(in crate::syntax) const fn green_node(&self) -> &GreenNode {
        &self.node
    }

    pub(in crate::syntax) const fn start(&self) -> TextSize {
        self.start
    }

    pub(in crate::syntax) fn child_nodes(
        &self,
        kind: SyntaxKind,
    ) -> impl Iterator<Item = (GreenNode, TextSize)> + '_ {
        self.node.child_nodes(self.start, kind)
    }

    pub(in crate::syntax) fn separator_tokens(
        &self,
        separator_kind: SyntaxKind,
    ) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(move |token| token.kind() == separator_kind)
    }

    pub(in crate::syntax) fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node.syntax_tokens(self.start)
    }

    pub(in crate::syntax) fn skipped_syntax(&self) -> impl Iterator<Item = SkippedSyntax> + '_ {
        self.node
            .skipped_syntax_nodes(self.start)
            .map(|(node, start)| {
                // SourceSnapshot clones share immutable source text with typed recovery nodes.
                SkippedSyntax::from_green(self.source.clone(), node, start)
            })
    }

    pub(in crate::syntax) fn into_green(self) -> GreenNode {
        self.node
    }
}

#[derive(Debug)]
pub(in crate::syntax) struct SeparatedSyntaxListBuilder {
    node: GreenNodeBuilder,
    list_kind: SyntaxKind,
    separator_kind: SyntaxKind,
}

impl SeparatedSyntaxListBuilder {
    pub(in crate::syntax) fn new(list_kind: SyntaxKind, separator_kind: SyntaxKind) -> Self {
        assert!(list_kind.is_node(), "separated list kind must be a node");
        assert!(
            separator_kind.is_token(),
            "separated list separator kind must be a token"
        );

        Self {
            node: GreenNodeBuilder::new(),
            list_kind,
            separator_kind,
        }
    }

    pub(in crate::syntax) fn push_item_node(&mut self, node: GreenNode) {
        self.node.push_node(node);
    }

    pub(in crate::syntax) fn push_separator_token(&mut self, token: SyntaxToken) {
        require_token_kind(
            token.kind(),
            self.separator_kind,
            "separated_list.separator_token",
        );

        self.node.push_token(token);
    }

    pub(in crate::syntax) fn push_skipped_tokens(
        &mut self,
        tokens: impl IntoIterator<Item = SyntaxToken>,
    ) {
        self.node.push_skipped_tokens(tokens);
    }

    pub(in crate::syntax) fn build(self) -> GreenNode {
        self.node.build(self.list_kind)
    }
}
