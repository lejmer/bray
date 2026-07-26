use bray_syntax::{
    GeneralGeneratorExpressionSyntax, GeneratorIterationExpressionSyntax, SyntaxKind,
};

use crate::parser::state::Parser;

impl Parser {
    pub(in crate::parser::expression) fn parse_general_generator_expression(
        &mut self,
    ) -> GeneralGeneratorExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = GeneralGeneratorExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_open_brace_token(self.expect(SyntaxKind::OpenBraceToken));
        builder.push_generator_iteration_expression(self.parse_generator_iteration_expression());

        self.recover_until(
            &mut builder,
            &[SyntaxKind::CloseBraceToken, SyntaxKind::EndOfFileToken],
        );

        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));

        builder.build()
    }

    pub(in crate::parser) fn parse_generator_iteration_expression(
        &mut self,
    ) -> GeneratorIterationExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = GeneratorIterationExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_each_keyword(self.expect(SyntaxKind::EachKeyword));

        let mut at_pattern_boundary = Parser::at_iteration_pattern_boundary;

        builder.push_irrefutable_pattern(
            self.parse_irrefutable_pattern_until(&mut at_pattern_boundary),
        );

        builder.push_in_keyword(self.expect(SyntaxKind::InKeyword));
        builder.push_iteration_source(self.parse_iteration_source());

        builder.push_block_expression(self.parse_flow_block_expression());

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::SyntaxText;

    use crate::parser::expression::test_support::parse_expression_until_semicolon_for_test;
    use crate::test_support::diagnostic_kinds;

    #[test]
    fn parser_parses_general_generator_expression() {
        let (expression, diagnostics) =
            parse_expression_until_semicolon_for_test("{ each item in items { yield item; } };");

        let primary = match expression.primary_expression() {
            Some(primary) => primary,
            None => panic!("expected primary"),
        };

        assert_eq!(
            expression.full_text(),
            "{ each item in items { yield item; } }"
        );

        assert!(primary.general_generator_expressions().next().is_some());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_array_generators_and_generator_block_items() {
        let cases = [
            (
                "[each item in items { yield item; }];",
                "[each item in items { yield item; }]",
            ),
            (
                "{ value; each item in items { yield item; } };",
                "{ value; each item in items { yield item; } }",
            ),
        ];

        for (source_text, expected_text) in cases {
            let (expression, diagnostics) = parse_expression_until_semicolon_for_test(source_text);

            assert_eq!(expression.full_text(), expected_text, "{source_text}");
            assert!(has_generator_iteration(&expression), "{source_text}");
            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }

    #[test]
    fn parser_recovers_bad_generator_tail_tokens_before_closing_delimiters() {
        let cases = [
            "{ each item in items { yield item; } === };",
            "[each item in items { yield item; } ===];",
        ];

        for source_text in cases {
            let (expression, diagnostics) = parse_expression_until_semicolon_for_test(source_text);

            let skipped = generator_skipped_texts(&expression);

            assert!(
                skipped
                    .iter()
                    .any(|skipped_syntax| skipped_syntax.contains("===")),
                "{source_text}: {skipped:?}"
            );

            assert_eq!(
                diagnostic_kinds(&diagnostics),
                [DiagnosticKind::LexicalInvalidOperatorOrPunctuation],
                "{source_text}"
            );
        }
    }

    fn has_generator_iteration(expression: &bray_syntax::ExpressionSyntax) -> bool {
        let Some(primary) = expression.primary_expression() else {
            return false;
        };

        primary
            .array_expressions()
            .any(|array| array.generator_iteration_expressions().next().is_some())
            || primary.block_expression().is_some_and(|block| {
                block
                    .block_items()
                    .any(|item| item.generator_iteration_expression().is_some())
            })
    }

    fn generator_skipped_texts(expression: &bray_syntax::ExpressionSyntax) -> Vec<String> {
        let Some(primary) = expression.primary_expression() else {
            return Vec::new();
        };

        let mut skipped_texts = Vec::new();

        for generator in primary.general_generator_expressions() {
            skipped_texts.extend(
                generator
                    .skipped_syntax()
                    .map(|skipped_syntax| skipped_syntax.full_text()),
            );
        }

        for array in primary.array_expressions() {
            skipped_texts.extend(
                array
                    .skipped_syntax()
                    .map(|skipped_syntax| skipped_syntax.full_text()),
            );
        }

        skipped_texts
    }
}
