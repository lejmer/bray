use bray_syntax::{SyntaxKind, TypeFormConstructionExpressionSyntax};

use crate::parser::state::Parser;

impl Parser {
    pub(in crate::parser::expression) fn parse_type_form_construction_expression(
        &mut self,
    ) -> TypeFormConstructionExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            TypeFormConstructionExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_box_keyword(self.expect(SyntaxKind::BoxKeyword));

        if self.at(SyntaxKind::OpenBracketToken) {
            builder.push_type_form_argument_list(self.parse_type_form_argument_list());
        }

        builder.push_argument_list(self.parse_argument_list());

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::{SyntaxKind, SyntaxText, TypeFormConstructionExpressionSyntax};

    use super::super::test_support::parse_expression_until_semicolon_for_test;

    #[test]
    fn parser_parses_type_form_construction_expressions() {
        let cases = [
            ("box(value);", "box(value)", false),
            ("box[Heap, 1](value);", "box[Heap, 1](value)", true),
        ];

        for (source_text, expected_text, has_type_arguments) in cases {
            let (expression, diagnostics) = parse_expression_until_semicolon_for_test(source_text);

            let construction = expression
                .primary_expression()
                .and_then(|primary| primary.type_form_construction_expressions().next());

            let construction = match construction {
                Some(construction) => construction,
                None => panic!("expected type-form construction expression: {source_text}"),
            };

            assert_eq!(expression.full_text(), expected_text, "{source_text}");

            assert_eq!(
                construction.type_form_argument_list().is_some(),
                has_type_arguments,
                "{source_text}"
            );

            assert_has_argument_list(&construction, source_text);

            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }

    fn assert_has_argument_list(
        construction: &TypeFormConstructionExpressionSyntax,
        source_text: &str,
    ) {
        assert_eq!(
            construction.argument_list().open_paren_token().kind(),
            SyntaxKind::OpenParenToken,
            "{source_text}"
        );
    }
}
