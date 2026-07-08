use crate::node::{child_nodes, define_source_syntax_node};
use crate::{BlockExpressionSyntax, SyntaxKind, SyntaxToken};

pub(super) fn first_expression(
    source: &bray_source::SourceSnapshot,
    node: &crate::green::GreenNode,
    start: bray_source::TextSize,
) -> Option<ExpressionSyntax> {
    child_nodes(
        source,
        node,
        start,
        SyntaxKind::Expression,
        ExpressionSyntax::from_green,
    )
    .next()
}

define_source_syntax_node! {
    /// Runtime expression.
    pub struct ExpressionSyntax {
        builder: ExpressionSyntaxBuilder,
        kind: SyntaxKind::Expression,
        source_slot: "expression.source",
        node_name: "expression",
        range_description: "expression",
        debug_name: "ExpressionSyntax",
        builder_debug_name: "ExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns direct nested expression children in source order.
                expressions;
                /// Appends a nested expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            },
            {
                /// Returns direct primary-expression children in source order.
                primary_expressions;
                /// Appends a primary-expression child.
                push_primary_expression;
                ty: PrimaryExpressionSyntax;
                kind: SyntaxKind::PrimaryExpression;
            }
        ],
    }
}

impl ExpressionSyntax {
    /// Returns the operator token when this expression is operator-shaped.
    pub fn operator_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| is_expression_operator(token.kind()))
    }

    /// Returns the primary-expression child when this is a primary expression.
    pub fn primary_expression(&self) -> Option<PrimaryExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::PrimaryExpression,
            PrimaryExpressionSyntax::from_green,
        )
        .next()
    }
}

impl ExpressionSyntaxBuilder {
    /// Appends an expression operator token.
    pub fn push_operator_token(&mut self, token: SyntaxToken) {
        assert!(
            is_expression_operator(token.kind()),
            "expression.operator_token expected an expression operator"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Opaque primary expression.
    pub struct PrimaryExpressionSyntax {
        builder: PrimaryExpressionSyntaxBuilder,
        kind: SyntaxKind::PrimaryExpression,
        source_slot: "primary_expression.source",
        node_name: "primary expression",
        range_description: "primary-expression",
        debug_name: "PrimaryExpressionSyntax",
        builder_debug_name: "PrimaryExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns nested expression children in source order.
                expressions;
                /// Appends a nested expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            },
            {
                /// Returns block-expression children in source order.
                block_expressions;
                /// Appends a block-expression child.
                push_block_expression;
                ty: BlockExpressionSyntax;
                kind: SyntaxKind::BlockExpression;
            }
        ],
    }
}

impl PrimaryExpressionSyntax {
    /// Returns the first direct primary token.
    pub fn primary_token(&self) -> Option<SyntaxToken> {
        self.tokens().next()
    }

    /// Returns the block-expression child when this is a block primary.
    pub fn block_expression(&self) -> Option<BlockExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::BlockExpression,
            BlockExpressionSyntax::from_green,
        )
        .next()
    }
}

impl PrimaryExpressionSyntaxBuilder {
    /// Appends a token owned by the primary-expression shell.
    pub fn push_token(&mut self, token: SyntaxToken) {
        assert!(
            is_primary_expression_token(token.kind()),
            "primary_expression.token expected a primary-expression token"
        );

        self.node.push_token(token);
    }
}

fn is_expression_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::EqualsToken
            | SyntaxKind::PipePipeToken
            | SyntaxKind::AmpersandAmpersandToken
            | SyntaxKind::EqualsEqualsToken
            | SyntaxKind::BangEqualsToken
            | SyntaxKind::LessToken
            | SyntaxKind::LessEqualsToken
            | SyntaxKind::GreaterToken
            | SyntaxKind::GreaterEqualsToken
            | SyntaxKind::PipeToken
            | SyntaxKind::CaretToken
            | SyntaxKind::AmpersandToken
            | SyntaxKind::LessLessToken
            | SyntaxKind::GreaterGreaterToken
            | SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::StarToken
            | SyntaxKind::SlashToken
            | SyntaxKind::PercentToken
            | SyntaxKind::AtToken
            | SyntaxKind::StarStarToken
            | SyntaxKind::TildeToken
            | SyntaxKind::BangToken
    )
}

fn is_primary_expression_token(kind: SyntaxKind) -> bool {
    kind.is_token()
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{keyword, snapshot as test_snapshot, token};
    use crate::{ExpressionSyntax, PrimaryExpressionSyntax, SyntaxKind, SyntaxText};

    #[test]
    fn expressions_store_operator_children_and_exact_text() {
        let snapshot = test_snapshot("syntax-expression-test", "left + right");

        let mut left = PrimaryExpressionSyntax::builder(snapshot.clone(), TextSize::new(0));

        left.push_token(keyword(SyntaxKind::IdentifierToken, 0, 4, true));

        let mut left_expression = ExpressionSyntax::builder(snapshot.clone(), TextSize::new(0));

        left_expression.push_primary_expression(left.build());

        let mut right = PrimaryExpressionSyntax::builder(snapshot.clone(), TextSize::new(7));

        right.push_token(token(SyntaxKind::IdentifierToken, 7, 12));

        let mut right_expression = ExpressionSyntax::builder(snapshot.clone(), TextSize::new(7));

        right_expression.push_primary_expression(right.build());

        let mut builder = ExpressionSyntax::builder(snapshot, TextSize::new(0));

        builder.push_expression(left_expression.build());
        builder.push_operator_token(keyword(SyntaxKind::PlusToken, 5, 6, true));
        builder.push_expression(right_expression.build());

        let expression = builder.build();
        let children = expression.expressions().collect::<Vec<_>>();

        assert_eq!(expression.full_text(), "left + right");
        assert_eq!(
            expression.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::PlusToken)
        );
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn primary_expressions_store_skipped_recovery() {
        let snapshot = test_snapshot("syntax-primary-expression-test", "target.call()");
        let mut builder = PrimaryExpressionSyntax::builder(snapshot, TextSize::new(0));

        builder.push_token(token(SyntaxKind::IdentifierToken, 0, 6));
        builder.push_skipped_tokens([
            token(SyntaxKind::DotToken, 6, 7),
            token(SyntaxKind::IdentifierToken, 7, 11),
            token(SyntaxKind::OpenParenToken, 11, 12),
            token(SyntaxKind::CloseParenToken, 12, 13),
        ]);

        let expression = builder.build();
        let skipped_syntax = expression.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected skipped primary-expression tail: {skipped_syntax:?}");
        };

        assert_eq!(expression.full_text(), "target.call()");
        assert_eq!(skipped.full_text(), ".call()");
    }
}
