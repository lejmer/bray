use bray_syntax::{BooleanFoldExpressionSyntax, SyntaxKind};

use crate::parser::state::Parser;

impl Parser {
    pub(in crate::parser::expression) fn parse_boolean_fold_expression(
        &mut self,
    ) -> BooleanFoldExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = BooleanFoldExpressionSyntax::builder(self.syntax_source(), start);

        match self.peek().kind() {
            SyntaxKind::AllKeyword => {
                builder.push_all_keyword(self.expect(SyntaxKind::AllKeyword));
            }
            SyntaxKind::AnyKeyword => {
                builder.push_any_keyword(self.expect(SyntaxKind::AnyKeyword));
            }
            _ => {
                builder.push_all_keyword(self.expect(SyntaxKind::AllKeyword));
            }
        }

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));

        let mut at_operand_boundary = Parser::at_boolean_fold_operand_boundary;

        builder.push_expression(self.parse_expression_until(&mut at_operand_boundary));
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn at_boolean_fold_operand_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CloseParenToken) || self.at(SyntaxKind::EndOfFileToken)
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::{SyntaxKind, SyntaxText};

    use super::super::test_support::parse_expression_until_semicolon_for_test;

    #[test]
    fn parser_parses_boolean_fold_expressions() {
        let cases = [
            ("all(values);", "all(values)", SyntaxKind::AllKeyword),
            ("any(values);", "any(values)", SyntaxKind::AnyKeyword),
        ];

        for (source_text, expected_text, expected_keyword) in cases {
            let (expression, diagnostics) = parse_expression_until_semicolon_for_test(source_text);

            let fold = expression
                .primary_expression()
                .and_then(|primary| primary.boolean_fold_expressions().next());

            let fold = match fold {
                Some(fold) => fold,
                None => panic!("expected boolean fold expression: {source_text}"),
            };

            assert_eq!(expression.full_text(), expected_text, "{source_text}");

            assert!(
                fold.expression().primary_expression().is_some(),
                "{source_text}"
            );

            assert!(
                fold.all_keyword()
                    .or_else(|| fold.any_keyword())
                    .is_some_and(|keyword| keyword.kind() == expected_keyword),
                "{source_text}"
            );

            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }
}
