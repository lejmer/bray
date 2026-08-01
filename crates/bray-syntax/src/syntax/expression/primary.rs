use crate::green::GreenElement;
use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    AbsenceExpressionSyntax, AccessExpressionSyntax, ArrayExpressionSyntax,
    AssertionExpressionSyntax, AwaitExpressionSyntax, BlockExpressionSyntax,
    BooleanFoldExpressionSyntax, BorrowExpressionSyntax, BreakExpressionSyntax,
    CatchExpressionSyntax, ConditionalExpressionSyntax, ContinueExpressionSyntax,
    ForExpressionSyntax, GeneralGeneratorExpressionSyntax, GroupedExpressionSyntax,
    LambdaExpressionSyntax, LeadingDotVariantExpressionSyntax, LiteralExpressionSyntax,
    LoopExpressionSyntax, MatchExpressionSyntax, PanicExpressionSyntax,
    ResultPropagationExpressionSyntax, ReturnExpressionSyntax, StructConstructionBodySyntax,
    SyntaxKind, SyntaxToken, TrustBoundaryExpressionSyntax, TupleExpressionSyntax,
    TypeFormConstructionExpressionSyntax, UnitExpressionSyntax, WhileExpressionSyntax,
    WithExpressionSyntax, YieldExpressionSyntax,
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
                /// Returns general-generator-expression children in source order.
                general_generator_expressions;
                /// Appends a general-generator-expression child.
                push_general_generator_expression;
                ty: GeneralGeneratorExpressionSyntax;
                kind: SyntaxKind::GeneralGeneratorExpression;
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
            },
            {
                /// Returns conditional-expression children in source order.
                conditional_expressions;
                /// Appends a conditional-expression child.
                push_conditional_expression;
                ty: ConditionalExpressionSyntax;
                kind: SyntaxKind::ConditionalExpression;
            },
            {
                /// Returns match-expression children in source order.
                match_expressions;
                /// Appends a match-expression child.
                push_match_expression;
                ty: MatchExpressionSyntax;
                kind: SyntaxKind::MatchExpression;
            },
            {
                /// Returns while-expression children in source order.
                while_expressions;
                /// Appends a while-expression child.
                push_while_expression;
                ty: WhileExpressionSyntax;
                kind: SyntaxKind::WhileExpression;
            },
            {
                /// Returns for-expression children in source order.
                for_expressions;
                /// Appends a for-expression child.
                push_for_expression;
                ty: ForExpressionSyntax;
                kind: SyntaxKind::ForExpression;
            },
            {
                /// Returns loop-expression children in source order.
                loop_expressions;
                /// Appends a loop-expression child.
                push_loop_expression;
                ty: LoopExpressionSyntax;
                kind: SyntaxKind::LoopExpression;
            },
            {
                /// Returns with-expression children in source order.
                with_expressions;
                /// Appends a with-expression child.
                push_with_expression;
                ty: WithExpressionSyntax;
                kind: SyntaxKind::WithExpression;
            },
            {
                /// Returns lambda-expression children in source order.
                lambda_expressions;
                /// Appends a lambda-expression child.
                push_lambda_expression;
                ty: LambdaExpressionSyntax;
                kind: SyntaxKind::LambdaExpression;
            },
            {
                /// Returns borrow-expression children in source order.
                borrow_expressions;
                /// Appends a borrow-expression child.
                push_borrow_expression;
                ty: BorrowExpressionSyntax;
                kind: SyntaxKind::BorrowExpression;
            },
            {
                /// Returns trust-boundary-expression children in source order.
                trust_boundary_expressions;
                /// Appends a trust-boundary-expression child.
                push_trust_boundary_expression;
                ty: TrustBoundaryExpressionSyntax;
                kind: SyntaxKind::TrustBoundaryExpression;
            },
            {
                /// Returns assertion-expression children in source order.
                assertion_expressions;
                /// Appends an assertion-expression child.
                push_assertion_expression;
                ty: AssertionExpressionSyntax;
                kind: SyntaxKind::AssertionExpression;
            },
            {
                /// Returns result-propagation-expression children in source order.
                result_propagation_expressions;
                /// Appends a result-propagation-expression child.
                push_result_propagation_expression;
                ty: ResultPropagationExpressionSyntax;
                kind: SyntaxKind::ResultPropagationExpression;
            },
            {
                /// Returns catch-expression children in source order.
                catch_expressions;
                /// Appends a catch-expression child.
                push_catch_expression;
                ty: CatchExpressionSyntax;
                kind: SyntaxKind::CatchExpression;
            },
            {
                /// Returns await-expression children in source order.
                await_expressions;
                /// Appends an await-expression child.
                push_await_expression;
                ty: AwaitExpressionSyntax;
                kind: SyntaxKind::AwaitExpression;
            },
            {
                /// Returns type-form-construction-expression children in source order.
                type_form_construction_expressions;
                /// Appends a type-form-construction-expression child.
                push_type_form_construction_expression;
                ty: TypeFormConstructionExpressionSyntax;
                kind: SyntaxKind::TypeFormConstructionExpression;
            },
            {
                /// Returns boolean-fold-expression children in source order.
                boolean_fold_expressions;
                /// Appends a boolean-fold-expression child.
                push_boolean_fold_expression;
                ty: BooleanFoldExpressionSyntax;
                kind: SyntaxKind::BooleanFoldExpression;
            },
            {
                /// Returns yield-expression children in source order.
                yield_expressions;
                /// Appends a yield-expression child.
                push_yield_expression;
                ty: YieldExpressionSyntax;
                kind: SyntaxKind::YieldExpression;
            },
            {
                /// Returns return-expression children in source order.
                return_expressions;
                /// Appends a return-expression child.
                push_return_expression;
                ty: ReturnExpressionSyntax;
                kind: SyntaxKind::ReturnExpression;
            },
            {
                /// Returns panic-expression children in source order.
                panic_expressions;
                /// Appends a panic-expression child.
                push_panic_expression;
                ty: PanicExpressionSyntax;
                kind: SyntaxKind::PanicExpression;
            },
            {
                /// Returns break-expression children in source order.
                break_expressions;
                /// Appends a break-expression child.
                push_break_expression;
                ty: BreakExpressionSyntax;
                kind: SyntaxKind::BreakExpression;
            },
            {
                /// Returns continue-expression children in source order.
                continue_expressions;
                /// Appends a continue-expression child.
                push_continue_expression;
                ty: ContinueExpressionSyntax;
                kind: SyntaxKind::ContinueExpression;
            }
        ],
    }
}

impl PrimaryExpressionSyntax {
    pub(crate) fn is_block_shaped(&self) -> bool {
        self.node.children().iter().any(|child| {
            matches!(
                child,
                GreenElement::Node(node) if node.kind().is_block_shaped_expression()
            )
        })
    }

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
