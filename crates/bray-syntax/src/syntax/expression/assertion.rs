use crate::node::define_source_syntax_node;
use crate::{ExpressionSyntax, SyntaxKind};

define_source_syntax_node! {
    /// Assertion expression.
    pub struct AssertionExpressionSyntax {
        builder: AssertionExpressionSyntaxBuilder,
        kind: SyntaxKind::AssertionExpression,
        source_slot: "assertion_expression.source",
        node_name: "assertion expression",
        range_description: "assertion-expression",
        debug_name: "AssertionExpressionSyntax",
        builder_debug_name: "AssertionExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `assert` keyword token.
                assert_keyword;
                /// Appends the `assert` keyword token.
                push_assert_keyword;
                kind: SyntaxKind::AssertKeyword;
                slot: "assertion_expression.assert_keyword";
            },
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "assertion_expression.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "assertion_expression.close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional comma token.
                comma_token;
                /// Appends the optional comma token.
                push_comma_token;
                kind: SyntaxKind::CommaToken;
                slot: "assertion_expression.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns assertion expression operands in source order.
                expressions;
                /// Appends an assertion expression operand.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl AssertionExpressionSyntax {
    /// Returns the assertion condition expression child.
    pub fn condition_expression(&self) -> Option<ExpressionSyntax> {
        self.expressions().next()
    }

    /// Returns the optional assertion message expression child.
    pub fn message_expression(&self) -> Option<ExpressionSyntax> {
        self.expressions().nth(1)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{keyword, snapshot as test_snapshot, token_expression};
    use crate::{AssertionExpressionSyntax, SyntaxKind, SyntaxText};

    #[test]
    fn assertion_expressions_store_condition_and_optional_message() {
        let snapshot = test_snapshot("syntax-assertion-expression-test", "assert(ok, message)");

        let mut builder = AssertionExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_assert_keyword(keyword(SyntaxKind::AssertKeyword, 0, 6, false));
        builder.push_open_paren_token(keyword(SyntaxKind::OpenParenToken, 6, 7, false));

        builder.push_expression(token_expression(
            snapshot.clone(),
            SyntaxKind::IdentifierToken,
            7,
            9,
        ));

        builder.push_comma_token(keyword(SyntaxKind::CommaToken, 9, 10, true));

        builder.push_expression(token_expression(
            snapshot.clone(),
            SyntaxKind::IdentifierToken,
            11,
            18,
        ));

        builder.push_close_paren_token(keyword(SyntaxKind::CloseParenToken, 18, 19, false));

        let expression = builder.build();

        assert_eq!(expression.full_text(), "assert(ok, message)");

        assert_eq!(
            expression
                .condition_expression()
                .map(|expression| expression.full_text()),
            Some(String::from("ok"))
        );

        assert_eq!(
            expression
                .message_expression()
                .map(|expression| expression.full_text()),
            Some(String::from("message"))
        );
    }
}
