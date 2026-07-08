use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    AbsenceExpressionSyntax, AccessExpressionSyntax, ArrayExpressionSyntax, BlockExpressionSyntax,
    GroupedExpressionSyntax, LeadingDotVariantExpressionSyntax, LiteralExpressionSyntax,
    StructConstructionBodySyntax, SyntaxKind, SyntaxToken, TupleExpressionSyntax,
    UnitExpressionSyntax,
};

define_source_syntax_node! {
    /// Primary expression wrapper.
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
                /// Returns access-expression children in source order.
                access_expressions;
                /// Appends an access-expression child.
                push_access_expression;
                ty: AccessExpressionSyntax;
                kind: SyntaxKind::AccessExpression;
            },
            {
                /// Returns struct-construction bodies in source order.
                struct_construction_bodies;
                /// Appends a struct-construction body child.
                push_struct_construction_body;
                ty: StructConstructionBodySyntax;
                kind: SyntaxKind::StructConstructionBody;
            },
            {
                /// Returns literal-expression children in source order.
                literal_expressions;
                /// Appends a literal-expression child.
                push_literal_expression;
                ty: LiteralExpressionSyntax;
                kind: SyntaxKind::LiteralExpression;
            },
            {
                /// Returns unit-expression children in source order.
                unit_expressions;
                /// Appends a unit-expression child.
                push_unit_expression;
                ty: UnitExpressionSyntax;
                kind: SyntaxKind::UnitExpression;
            },
            {
                /// Returns absence-expression children in source order.
                absence_expressions;
                /// Appends an absence-expression child.
                push_absence_expression;
                ty: AbsenceExpressionSyntax;
                kind: SyntaxKind::AbsenceExpression;
            },
            {
                /// Returns grouped-expression children in source order.
                grouped_expressions;
                /// Appends a grouped-expression child.
                push_grouped_expression;
                ty: GroupedExpressionSyntax;
                kind: SyntaxKind::GroupedExpression;
            },
            {
                /// Returns tuple-expression children in source order.
                tuple_expressions;
                /// Appends a tuple-expression child.
                push_tuple_expression;
                ty: TupleExpressionSyntax;
                kind: SyntaxKind::TupleExpression;
            },
            {
                /// Returns array-expression children in source order.
                array_expressions;
                /// Appends an array-expression child.
                push_array_expression;
                ty: ArrayExpressionSyntax;
                kind: SyntaxKind::ArrayExpression;
            },
            {
                /// Returns leading-dot variant-expression children in source order.
                leading_dot_variant_expressions;
                /// Appends a leading-dot variant-expression child.
                push_leading_dot_variant_expression;
                ty: LeadingDotVariantExpressionSyntax;
                kind: SyntaxKind::LeadingDotVariantExpression;
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

    /// Returns the access-expression child when this is access-shaped.
    pub fn access_expression(&self) -> Option<AccessExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::AccessExpression,
            AccessExpressionSyntax::from_green,
        )
        .next()
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

    /// Returns the struct-construction body child when present.
    pub fn struct_construction_body(&self) -> Option<StructConstructionBodySyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::StructConstructionBody,
            StructConstructionBodySyntax::from_green,
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

fn is_primary_expression_token(kind: SyntaxKind) -> bool {
    kind.is_token()
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{snapshot as test_snapshot, token, token_expression};
    use crate::{
        AccessExpressionSyntax, ArgumentListSyntax, ArgumentSyntax, ExpressionSyntax,
        MemberAccessOperationSyntax, PrimaryExpressionSyntax, SyntaxKind, SyntaxText,
    };

    #[test]
    fn primary_expressions_store_typed_roots_and_postfixes() {
        let snapshot = test_snapshot("syntax-primary-expression-test", "target.call(1)");
        let expression_source = snapshot.clone();

        let mut access = AccessExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);
        let mut member = MemberAccessOperationSyntax::builder(snapshot.clone(), TextSize::new(6));
        let mut list = ArgumentListSyntax::builder(snapshot.clone(), TextSize::new(11));
        let mut argument = ArgumentSyntax::builder(snapshot.clone(), TextSize::new(12));
        let mut primary = PrimaryExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);
        let mut call = crate::CallOperationSyntax::builder(snapshot.clone(), TextSize::new(11));
        let mut expression = ExpressionSyntax::builder(snapshot, TextSize::ZERO);

        access.push_identifier_token(token(SyntaxKind::IdentifierToken, 0, 6));

        member.push_dot_token(token(SyntaxKind::DotToken, 6, 7));
        member.push_identifier_token(token(SyntaxKind::IdentifierToken, 7, 11));
        access.push_member_access_operation(member.build());

        argument.push_expression(token_expression(
            expression_source,
            SyntaxKind::DecimalIntegerLiteralToken,
            12,
            13,
        ));

        list.push_open_paren_token(token(SyntaxKind::OpenParenToken, 11, 12));
        list.push_argument(argument.build());
        list.push_close_paren_token(token(SyntaxKind::CloseParenToken, 13, 14));

        primary.push_access_expression(access.build());
        call.push_argument_list(list.build());

        expression.push_primary_expression(primary.build());
        expression.push_call_operation(call.build());

        let expression = expression.build();

        assert_eq!(expression.full_text(), "target.call(1)");
        assert_eq!(expression.call_operations().count(), 1);
    }
}
