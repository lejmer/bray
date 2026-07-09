use bray_syntax::{LambdaExpressionSyntax, SyntaxKind};

use crate::parser::state::Parser;

const LAMBDA_EXPRESSION_START_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::LambdaKeyword,
];

const LAMBDA_CONTRACT_BOUNDARY_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(in crate::parser::expression) fn parse_lambda_expression(
        &mut self,
    ) -> LambdaExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = LambdaExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_callable_directives(
            self.parse_callable_directives_until(&LAMBDA_EXPRESSION_START_KINDS),
        );
        builder.push_callable_modifiers(self.parse_callable_modifiers());
        builder.push_lambda_keyword(self.expect(SyntaxKind::LambdaKeyword));
        builder.push_parameter_list(self.parse_parameter_list());

        if self.at(SyntaxKind::ArrowToken) {
            builder.push_callable_result_clause(self.parse_callable_result_clause());
        }

        self.parse_callable_contract_clauses(
            &mut builder,
            Parser::at_lambda_callable_contract_boundary,
        );

        builder.push_callable_body_block_expression(
            self.parse_callable_body_block_expression_until(Parser::at_flow_block_missing_boundary),
        );

        builder.build()
    }

    pub(in crate::parser::expression) fn should_parse_lambda_expression(&mut self) -> bool {
        if !self.at_any(&LAMBDA_EXPRESSION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_callable_directives_for_scan_until(&LAMBDA_EXPRESSION_START_KINDS);
            scan.consume_callable_modifiers_for_scan();

            scan.at(SyntaxKind::LambdaKeyword)
        })
    }

    fn at_lambda_callable_contract_boundary(&mut self) -> bool {
        self.at_callable_contract_clause_start() || self.at_any(&LAMBDA_CONTRACT_BOUNDARY_KINDS)
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{ExpressionSyntax, SyntaxText};

    use crate::parser::expression::test_support::parse_expression_until_semicolon_for_test;
    use crate::test_support::diagnostic_kinds;

    #[test]
    fn parser_parses_lambda_expression_callable_parts() {
        let source_text = concat!(
            "@abi(\"C\") async trusted const lambda(value: Int = 1) ",
            "-> Bool requires(value) uses(core.io) { return value; };"
        );

        let (expression, diagnostics) = parse_expression_until_semicolon_for_test(source_text);

        let lambda = match first_lambda_expression(&expression) {
            Some(lambda) => lambda,
            None => panic!("expected lambda expression"),
        };

        assert_eq!(
            expression.full_text(),
            concat!(
                "@abi(\"C\") async trusted const lambda(value: Int = 1) ",
                "-> Bool requires(value) uses(core.io) { return value; }"
            )
        );

        assert_eq!(lambda.callable_directives().abi_directives().count(), 1);
        assert!(lambda.callable_modifiers().async_token().is_some());
        assert!(lambda.callable_modifiers().trusted_token().is_some());
        assert!(lambda.callable_modifiers().const_token().is_some());
        assert_eq!(lambda.parameter_list().parameters().count(), 1);
        assert!(lambda.callable_result_clause().is_some());
        assert_eq!(lambda.requires_clauses().count(), 1);
        assert_eq!(lambda.uses_clauses().count(), 1);

        assert_eq!(
            lambda
                .callable_body_block_expression()
                .block_expression()
                .block_items()
                .count(),
            1
        );

        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn parser_parses_modifier_start_lambda_expressions() {
        let cases = [
            "async lambda() {};",
            "trusted lambda() {};",
            "const lambda() {};",
        ];

        for source_text in cases {
            let (expression, diagnostics) = parse_expression_until_semicolon_for_test(source_text);

            assert!(
                first_lambda_expression(&expression).is_some(),
                "{source_text}"
            );

            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }

    #[test]
    fn parser_reports_missing_lambda_body_without_consuming_semicolon() {
        let (expression, diagnostics) =
            parse_expression_until_semicolon_for_test("lambda() -> Bool;");

        let lambda = match first_lambda_expression(&expression) {
            Some(lambda) => lambda,
            None => panic!("expected lambda expression"),
        };

        assert_eq!(expression.full_text(), "lambda() -> Bool");

        assert!(
            lambda
                .callable_body_block_expression()
                .block_expression()
                .open_brace_token()
                .is_missing()
        );

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxExpectedToken
            ]
        );
    }

    fn first_lambda_expression(
        expression: &ExpressionSyntax,
    ) -> Option<bray_syntax::LambdaExpressionSyntax> {
        if let Some(lambda) = expression
            .primary_expression()
            .and_then(|primary| primary.lambda_expressions().next())
        {
            return Some(lambda);
        }

        expression
            .expressions()
            .find_map(|child| first_lambda_expression(&child))
    }
}
