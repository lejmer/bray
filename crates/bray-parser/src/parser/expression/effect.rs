use bray_syntax::{
    AwaitExpressionSyntax, BorrowExpressionSyntax, CatchExpressionSyntax, ExpressionSyntax,
    ResultPropagationExpressionSyntax, SyntaxKind, TrustBoundaryExpressionSyntax,
};

use crate::parser::state::Parser;

impl Parser {
    pub(in crate::parser::expression) fn parse_borrow_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> BorrowExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = BorrowExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_ampersand_token(self.expect(SyntaxKind::AmpersandToken));

        if self.at(SyntaxKind::MutKeyword) {
            builder.push_mut_keyword(self.expect(SyntaxKind::MutKeyword));
        }

        builder.push_expression(self.parse_required_expression_operand_until(at_boundary));

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_trust_boundary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TrustBoundaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TrustBoundaryExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_trusted_keyword(self.expect(SyntaxKind::TrustedKeyword));
        builder.push_expression(self.parse_required_expression_operand_until(at_boundary));

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_result_propagation_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ResultPropagationExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ResultPropagationExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_try_keyword(self.expect(SyntaxKind::TryKeyword));
        builder.push_expression(self.parse_required_expression_operand_until(at_boundary));

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_catch_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> CatchExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = CatchExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_catch_keyword(self.expect(SyntaxKind::CatchKeyword));
        builder.push_expression(self.parse_required_expression_operand_until(at_boundary));

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_await_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> AwaitExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = AwaitExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_await_keyword(self.expect(SyntaxKind::AwaitKeyword));
        builder.push_expression(self.parse_required_expression_operand_until(at_boundary));

        builder.build()
    }

    fn parse_required_expression_operand_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        if self.at_expression_operand_boundary(at_boundary) {
            return self.missing_expression();
        }

        self.parse_expression_until(at_boundary)
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::{SyntaxKind, SyntaxText};

    use super::super::test_support::{
        parse_expression_until_semicolon_for_test, primary_contains_child_kind_for_test,
    };

    #[test]
    fn parser_parses_prefix_effect_primary_roots() {
        let cases = [
            ("&value;", "&value", SyntaxKind::BorrowExpression),
            ("& mut value;", "& mut value", SyntaxKind::BorrowExpression),
            (
                "trusted value;",
                "trusted value",
                SyntaxKind::TrustBoundaryExpression,
            ),
            (
                "try value;",
                "try value",
                SyntaxKind::ResultPropagationExpression,
            ),
            ("catch task;", "catch task", SyntaxKind::CatchExpression),
            ("await task;", "await task", SyntaxKind::AwaitExpression),
        ];

        for (source_text, expected_text, expected_kind) in cases {
            let (expression, diagnostics) = parse_expression_until_semicolon_for_test(source_text);

            assert_eq!(expression.full_text(), expected_text, "{source_text}");

            assert!(
                expression.primary_expression().is_some_and(|primary| {
                    primary_contains_child_kind_for_test(&primary, expected_kind)
                }),
                "{source_text}"
            );

            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }
}
