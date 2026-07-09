use bray_syntax::{AssertionExpressionSyntax, SyntaxKind};

use crate::parser::state::Parser;

impl Parser {
    pub(in crate::parser::expression) fn parse_assertion_expression(
        &mut self,
    ) -> AssertionExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = AssertionExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_assert_keyword(self.expect(SyntaxKind::AssertKeyword));
        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));

        let mut at_condition_boundary = Parser::at_assertion_condition_boundary;

        builder.push_expression(
            self.parse_non_assignment_expression_until(&mut at_condition_boundary),
        );

        if self.at(SyntaxKind::CommaToken) {
            builder.push_comma_token(self.expect(SyntaxKind::CommaToken));

            let mut at_message_boundary = Parser::at_assertion_message_boundary;

            builder.push_expression(self.parse_expression_until(&mut at_message_boundary));
        }

        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn at_assertion_condition_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CommaToken)
            || self.at(SyntaxKind::CloseParenToken)
            || self.at(SyntaxKind::EndOfFileToken)
    }

    fn at_assertion_message_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CloseParenToken) || self.at(SyntaxKind::EndOfFileToken)
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::{SyntaxKind, SyntaxText};

    use super::super::test_support::parse_expression_until_semicolon_for_test;

    #[test]
    fn parser_parses_assertion_expressions() {
        let cases = [
            ("assert(ok);", "assert(ok)", 1),
            ("assert(ok, message);", "assert(ok, message)", 2),
        ];

        for (source_text, expected_text, expected_expression_count) in cases {
            let (expression, diagnostics) = parse_expression_until_semicolon_for_test(source_text);

            let assertion = expression
                .primary_expression()
                .and_then(|primary| primary.assertion_expressions().next());

            let assertion = match assertion {
                Some(assertion) => assertion,
                None => panic!("expected assertion expression: {source_text}"),
            };

            assert_eq!(expression.full_text(), expected_text, "{source_text}");
            assert_eq!(assertion.expressions().count(), expected_expression_count);

            assert_eq!(
                assertion.assert_keyword().kind(),
                SyntaxKind::AssertKeyword,
                "{source_text}"
            );

            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }
}
