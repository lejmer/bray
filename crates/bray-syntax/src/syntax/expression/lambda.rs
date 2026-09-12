use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    CallableBodyBlockExpressionSyntax, CallableDirectivesSyntax, CallableModifiersSyntax,
    CallableResultClauseSyntax, EnsuresClauseSyntax, ParameterListSyntax, RequiresClauseSyntax,
    SyntaxKind, UsesClauseSyntax, WithClauseSyntax,
};

define_source_syntax_node! {
    /// Lambda expression.
    pub struct LambdaExpressionSyntax {
        builder: LambdaExpressionSyntaxBuilder,
        kind: SyntaxKind::LambdaExpression,
        source_slot: "lambda_expression.source",
        node_name: "lambda expression",
        range_description: "lambda-expression",
        debug_name: "LambdaExpressionSyntax",
        builder_debug_name: "LambdaExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `lambda` keyword token.
                lambda_keyword;
                /// Appends the `lambda` keyword token.
                push_lambda_keyword;
                kind: SyntaxKind::LambdaKeyword;
                slot: "lambda_expression.lambda_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the callable-directives child.
                callable_directives;
                /// Appends the callable-directives child.
                push_callable_directives;
                ty: CallableDirectivesSyntax;
                kind: SyntaxKind::CallableDirectives;
            },
            {
                /// Returns the callable-modifiers child.
                callable_modifiers;
                /// Appends the callable-modifiers child.
                push_callable_modifiers;
                ty: CallableModifiersSyntax;
                kind: SyntaxKind::CallableModifiers;
            },
            {
                /// Returns the parameter-list child.
                parameter_list;
                /// Appends the parameter-list child.
                push_parameter_list;
                ty: ParameterListSyntax;
                kind: SyntaxKind::ParameterList;
            },
            {
                /// Returns the callable body block expression.
                callable_body_block_expression;
                /// Appends the callable body block expression.
                push_callable_body_block_expression;
                ty: CallableBodyBlockExpressionSyntax;
                kind: SyntaxKind::CallableBodyBlockExpression;
            }
        ],
        repeated_children: [
            {
                /// Returns callable result clauses in source order.
                callable_result_clauses;
                /// Appends a callable result clause child.
                push_callable_result_clause;
                ty: CallableResultClauseSyntax;
                kind: SyntaxKind::CallableResultClause;
            },
            {
                /// Returns `requires(...)` clauses in source order.
                requires_clauses;
                /// Appends a `requires(...)` clause.
                push_requires_clause;
                ty: RequiresClauseSyntax;
                kind: SyntaxKind::RequiresClause;
            },
            {
                /// Returns `ensures(...)` clauses in source order.
                ensures_clauses;
                /// Appends an `ensures(...)` clause.
                push_ensures_clause;
                ty: EnsuresClauseSyntax;
                kind: SyntaxKind::EnsuresClause;
            },
            {
                /// Returns `executes(...)` clauses in source order.
                executes_clauses;
                /// Appends an `executes(...)` clause.
                push_executes_clause;
                ty: crate::ExecutesClauseSyntax;
                kind: SyntaxKind::ExecutesClause;
            },
            {
                /// Returns `when(...)` clauses in source order.
                when_clauses;
                /// Appends a `when(...)` clause.
                push_when_clause;
                ty: crate::WhenClauseSyntax;
                kind: SyntaxKind::WhenClause;
            },
            {
                /// Returns `with(...)` clauses in source order.
                with_clauses;
                /// Appends a `with(...)` clause.
                push_with_clause;
                ty: WithClauseSyntax;
                kind: SyntaxKind::WithClause;
            },
            {
                /// Returns `uses(...)` clauses in source order.
                uses_clauses;
                /// Appends a `uses(...)` clause.
                push_uses_clause;
                ty: UsesClauseSyntax;
                kind: SyntaxKind::UsesClause;
            }
        ],
    }
}

impl LambdaExpressionSyntax {
    /// Returns the callable result clause child when present.
    pub fn callable_result_clause(&self) -> Option<CallableResultClauseSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::CallableResultClause,
            CallableResultClauseSyntax::from_green,
        )
        .next()
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use crate::test_support::{
        callable_body_block_expression, keyword, snapshot as test_snapshot, token,
    };
    use crate::{
        CallableDirectivesSyntax, CallableModifiersSyntax, LambdaExpressionSyntax,
        ParameterListSyntax, SyntaxKind, SyntaxText, SyntaxTrivia,
    };

    #[test]
    fn lambda_expressions_store_callable_children_and_body() {
        let snapshot = test_snapshot("syntax-lambda-expression-test", "async lambda() {}");

        let mut modifiers = CallableModifiersSyntax::builder(snapshot.clone(), TextSize::ZERO);

        modifiers.push_async_token(keyword(SyntaxKind::AsyncKeyword, 0, 5, true));

        let mut parameters = ParameterListSyntax::builder(snapshot.clone(), TextSize::new(13));

        parameters.push_open_paren_token(token(SyntaxKind::OpenParenToken, 13, 14));

        parameters.push_close_paren_token(
            token(SyntaxKind::CloseParenToken, 14, 15).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(15), TextSize::new(16))),
            ]),
        );

        let mut builder = LambdaExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_callable_directives(
            CallableDirectivesSyntax::builder(snapshot.clone(), TextSize::ZERO).build(),
        );

        builder.push_callable_modifiers(modifiers.build());
        builder.push_lambda_keyword(keyword(SyntaxKind::LambdaKeyword, 6, 12, false));
        builder.push_parameter_list(parameters.build());

        builder.push_callable_body_block_expression(callable_body_block_expression(
            snapshot, 16, false,
        ));

        let expression = builder.build();

        assert_eq!(expression.full_text(), "async lambda() {}");
        assert!(expression.callable_modifiers().async_token().is_some());
        assert_eq!(expression.parameter_list().full_text(), "() ");
        assert!(expression.callable_result_clause().is_none());

        assert_eq!(
            expression.callable_body_block_expression().full_text(),
            "{}"
        );
    }
}
