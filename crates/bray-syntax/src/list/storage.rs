use bray_source::{SourceSnapshot, TextSize};

use crate::builder::{GreenNodeBuilder, require_token_kind};
use crate::green::GreenNode;
use crate::syntax::skipped_syntax_nodes;
use crate::{SkippedSyntax, SyntaxKind, SyntaxToken};

#[derive(Clone, Eq, Hash, PartialEq)]
pub(crate) struct SyntaxList {
    source: SourceSnapshot,
    node: GreenNode,
    start: TextSize,
}

impl SyntaxList {
    pub(crate) fn from_green(
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

    pub(crate) fn is_recovered(&self) -> bool {
        self.node.contains_recovery(self.start)
    }

    pub(crate) const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    pub(crate) const fn green_node(&self) -> &GreenNode {
        &self.node
    }

    pub(crate) const fn start(&self) -> TextSize {
        self.start
    }

    pub(crate) fn child_nodes(
        &self,
        kind: SyntaxKind,
    ) -> impl Iterator<Item = (GreenNode, TextSize)> + '_ {
        self.node.child_nodes(self.start, kind)
    }

    pub(crate) fn separator_tokens(
        &self,
        separator_kind: SyntaxKind,
    ) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(move |token| token.kind() == separator_kind)
    }

    pub(crate) fn tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.node.syntax_tokens(self.start)
    }

    pub(crate) fn skipped_syntax(&self) -> impl Iterator<Item = SkippedSyntax> + '_ {
        skipped_syntax_nodes(&self.source, &self.node, self.start)
    }

    pub(crate) fn into_green(self) -> GreenNode {
        self.node
    }
}

#[derive(Debug)]
pub(crate) struct SyntaxListBuilder {
    node: GreenNodeBuilder,
    list_kind: SyntaxKind,
    separator_kind: Option<SyntaxKind>,
}

impl SyntaxListBuilder {
    pub(crate) fn new(list_kind: SyntaxKind) -> Self {
        assert!(list_kind.is_node(), "list kind must be a node");

        Self {
            node: GreenNodeBuilder::new(),
            list_kind,
            separator_kind: None,
        }
    }

    pub(crate) fn with_separator(list_kind: SyntaxKind, separator_kind: SyntaxKind) -> Self {
        assert!(
            separator_kind.is_token(),
            "list separator kind must be a token"
        );

        let mut list = Self::new(list_kind);

        list.separator_kind = Some(separator_kind);

        list
    }

    pub(crate) fn push_item_node(&mut self, node: GreenNode) {
        self.node.push_node(node);
    }

    pub(crate) fn push_separator_token(&mut self, token: SyntaxToken) {
        let Some(separator_kind) = self.separator_kind else {
            panic!("list builder does not accept separator tokens");
        };

        require_token_kind(token.kind(), separator_kind, "list.separator_token");

        self.node.push_token(token);
    }

    pub(crate) fn push_skipped_tokens(&mut self, tokens: impl IntoIterator<Item = SyntaxToken>) {
        self.node.push_skipped_tokens(tokens);
    }

    pub(crate) fn build(self) -> GreenNode {
        self.node.build(self.list_kind)
    }
}
