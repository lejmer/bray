use crate::node::define_source_syntax_node;
use crate::{ExpressionSyntax, SyntaxKind};

define_source_syntax_node! {
    /// Boolean fold expression.
    pub struct BooleanFoldExpressionSyntax {
        builder: BooleanFoldExpressionSyntaxBuilder,
        kind: SyntaxKind::BooleanFoldExpression,
        source_slot: "boolean_fold_expression.source",
        node_name: "boolean fold expression",
        range_description: "boolean-fold-expression",
        debug_name: "BooleanFoldExpressionSyntax",
        builder_debug_name: "BooleanFoldExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "boolean_fold_expression.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "boolean_fold_expression.close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional `all` keyword token.
                all_keyword;
                /// Appends the optional `all` keyword token.
                push_all_keyword;
                kind: SyntaxKind::AllKeyword;
                slot: "boolean_fold_expression.all_keyword";
            },
            {
                /// Returns the optional `any` keyword token.
                any_keyword;
                /// Appends the optional `any` keyword token.
                push_any_keyword;
                kind: SyntaxKind::AnyKeyword;
                slot: "boolean_fold_expression.any_keyword";
            }
        ],
        required_children: [
            {
                /// Returns the boolean fold operand expression child.
                expression;
                /// Appends the boolean fold operand expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{keyword, snapshot as test_snapshot, token_expression};
    use crate::{BooleanFoldExpressionSyntax, SyntaxKind, SyntaxText};

    #[test]
    fn boolean_fold_expressions_store_keyword_and_operand() {
        let snapshot = test_snapshot("syntax-boolean-fold-expression-test", "all(value)");

        let mut builder = BooleanFoldExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_all_keyword(keyword(SyntaxKind::AllKeyword, 0, 3, false));
        builder.push_open_paren_token(keyword(SyntaxKind::OpenParenToken, 3, 4, false));

        builder.push_expression(token_expression(
            snapshot.clone(),
            SyntaxKind::IdentifierToken,
            4,
            9,
        ));

        builder.push_close_paren_token(keyword(SyntaxKind::CloseParenToken, 9, 10, false));

        let expression = builder.build();

        assert_eq!(expression.full_text(), "all(value)");
        assert!(expression.all_keyword().is_some());
        assert!(expression.any_keyword().is_none());
    }
}
