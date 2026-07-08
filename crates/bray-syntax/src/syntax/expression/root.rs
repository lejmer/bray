use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    CallOperationSyntax, ConversionOperationSyntax, ElementIndexOperationSyntax,
    MemberAccessOperationSyntax, NullablePropagationOperationSyntax, PrimaryExpressionSyntax,
    SliceIndexOperationSyntax, SyntaxKind, SyntaxToken, TraitQualifiedMemberOperationSyntax,
};

pub(in crate::syntax) fn first_expression(
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
            },
            {
                /// Returns member-access postfix operations in source order.
                member_access_operations;
                /// Appends a member-access postfix operation.
                push_member_access_operation;
                ty: MemberAccessOperationSyntax;
                kind: SyntaxKind::MemberAccessOperation;
            },
            {
                /// Returns element-index postfix operations in source order.
                element_index_operations;
                /// Appends an element-index postfix operation.
                push_element_index_operation;
                ty: ElementIndexOperationSyntax;
                kind: SyntaxKind::ElementIndexOperation;
            },
            {
                /// Returns call postfix operations in source order.
                call_operations;
                /// Appends a call postfix operation.
                push_call_operation;
                ty: CallOperationSyntax;
                kind: SyntaxKind::CallOperation;
            },
            {
                /// Returns slice-index postfix operations in source order.
                slice_index_operations;
                /// Appends a slice-index postfix operation.
                push_slice_index_operation;
                ty: SliceIndexOperationSyntax;
                kind: SyntaxKind::SliceIndexOperation;
            },
            {
                /// Returns nullable-propagation postfix operations in source order.
                nullable_propagation_operations;
                /// Appends a nullable-propagation postfix operation.
                push_nullable_propagation_operation;
                ty: NullablePropagationOperationSyntax;
                kind: SyntaxKind::NullablePropagationOperation;
            },
            {
                /// Returns conversion postfix operations in source order.
                conversion_operations;
                /// Appends a conversion postfix operation.
                push_conversion_operation;
                ty: ConversionOperationSyntax;
                kind: SyntaxKind::ConversionOperation;
            },
            {
                /// Returns trait-qualified member postfix operations in source order.
                trait_qualified_member_operations;
                /// Appends a trait-qualified member postfix operation.
                push_trait_qualified_member_operation;
                ty: TraitQualifiedMemberOperationSyntax;
                kind: SyntaxKind::TraitQualifiedMemberOperation;
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
}
