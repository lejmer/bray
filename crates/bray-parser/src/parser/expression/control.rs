use bray_syntax::{
    BlockExpressionSyntax, ConditionalElseSyntax, ConditionalExpressionSyntax, ExpressionSyntax,
    ForExpressionSyntax, IterationSourceSyntax, LoopExpressionSyntax, SyntaxKind,
    WhileExpressionSyntax,
};

use crate::parser::state::Parser;

impl Parser {
    pub(in crate::parser::expression) fn parse_conditional_expression(
        &mut self,
    ) -> ConditionalExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ConditionalExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_if_keyword(self.expect(SyntaxKind::IfKeyword));

        builder.push_expression(self.parse_condition());

        self.recover_until_predicate(
            &mut builder,
            Parser::at_expression_before_block_recovery_boundary,
        );

        builder.push_block_expression(self.parse_flow_block_expression());

        if self.at(SyntaxKind::ElseKeyword) {
            builder.push_conditional_else(self.parse_conditional_else());
        }

        builder.build()
    }

    fn parse_conditional_else(&mut self) -> ConditionalElseSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ConditionalElseSyntax::builder(self.syntax_source(), start);

        builder.push_else_keyword(self.expect(SyntaxKind::ElseKeyword));

        if self.at(SyntaxKind::IfKeyword) {
            builder.push_conditional_expression(self.parse_conditional_expression());
        } else {
            builder.push_block_expression(self.parse_flow_block_expression());
        }

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_while_expression(
        &mut self,
    ) -> WhileExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = WhileExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_while_keyword(self.expect(SyntaxKind::WhileKeyword));

        builder.push_expression(self.parse_condition());

        self.recover_until_predicate(
            &mut builder,
            Parser::at_expression_before_block_recovery_boundary,
        );

        builder.push_block_expression(self.parse_flow_block_expression());

        if self.at(SyntaxKind::ElseKeyword) {
            builder.push_else_keyword(self.expect(SyntaxKind::ElseKeyword));
            builder.push_block_expression(self.parse_flow_block_expression());
        }

        builder.build()
    }

    fn parse_condition(&mut self) -> ExpressionSyntax {
        let (mut condition, binds) = self.parse_condition_chain(true);

        if binds && self.at(SyntaxKind::PipePipeToken) {
            let actual = self.peek();

            self.record_syntax_diagnostic(crate::diagnostic::expected_operator(
                &self.syntax_source(),
                SyntaxKind::AmpersandAmpersandToken,
                &actual,
            ));
        }

        while !binds && self.at(SyntaxKind::PipePipeToken) {
            let start = condition.full_range().start();
            let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

            builder.push_expression(condition);
            builder.push_operator_token(self.consume());
            builder.push_expression(self.parse_condition_chain(false).0);
            condition = builder.build();
        }

        condition
    }

    fn parse_condition_chain(&mut self, allow_binding: bool) -> (ExpressionSyntax, bool) {
        let mut binds = allow_binding && self.at(SyntaxKind::LetKeyword);
        let mut condition = self.parse_condition_operand(allow_binding);

        while self.at(SyntaxKind::AmpersandAmpersandToken) {
            let start = condition.full_range().start();
            let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

            builder.push_expression(condition);
            builder.push_operator_token(self.consume());
            binds |= allow_binding && self.at(SyntaxKind::LetKeyword);
            builder.push_expression(self.parse_condition_operand(allow_binding));
            condition = builder.build();
        }

        (condition, binds)
    }

    fn parse_condition_operand(&mut self, allow_binding: bool) -> ExpressionSyntax {
        let mut boundary = Parser::at_expression_before_block_boundary;

        if !allow_binding || !self.at(SyntaxKind::LetKeyword) {
            return self.parse_expression_with_min_binding_power_until(&mut boundary, 4);
        }

        let start = self.peek().full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_let_keyword(self.consume());

        builder.push_case_pattern(self.parse_case_pattern_until(&mut |parser| {
            parser.at(SyntaxKind::EqualsToken)
                || parser.at(SyntaxKind::EndOfFileToken)
                || parser.at(SyntaxKind::CloseBraceToken)
        }));

        builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));

        builder
            .push_expression(self.parse_expression_with_min_binding_power_until(&mut boundary, 4));

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_for_expression(&mut self) -> ForExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ForExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_for_keyword(self.expect(SyntaxKind::ForKeyword));

        let mut at_pattern_boundary = Parser::at_iteration_pattern_boundary;

        builder.push_irrefutable_pattern(
            self.parse_irrefutable_pattern_until(&mut at_pattern_boundary),
        );

        builder.push_in_keyword(self.expect(SyntaxKind::InKeyword));
        builder.push_iteration_source(self.parse_iteration_source());

        builder.push_block_expression(self.parse_flow_block_expression());

        if self.at(SyntaxKind::ElseKeyword) {
            builder.push_else_keyword(self.expect(SyntaxKind::ElseKeyword));
            builder.push_block_expression(self.parse_flow_block_expression());
        }

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_iteration_source(
        &mut self,
    ) -> IterationSourceSyntax {
        let start = self.peek().full_range().start();
        let mut builder = IterationSourceSyntax::builder(self.syntax_source(), start);

        match self.peek().kind() {
            SyntaxKind::MutKeyword => {
                builder.push_mut_keyword(self.expect(SyntaxKind::MutKeyword));
            }
            SyntaxKind::MoveKeyword => {
                builder.push_move_keyword(self.expect(SyntaxKind::MoveKeyword));
            }
            _ => {}
        }

        let mut at_source_boundary = Parser::at_expression_before_block_boundary;

        builder.push_expression(self.parse_expression_until(&mut at_source_boundary));

        self.recover_until_predicate(
            &mut builder,
            Parser::at_expression_before_block_recovery_boundary,
        );

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_flow_block_expression(
        &mut self,
    ) -> BlockExpressionSyntax {
        if !self.at(SyntaxKind::OpenBraceToken) && self.at_flow_block_missing_boundary() {
            return self.missing_block_expression();
        }

        let mut at_missing_body_boundary = Parser::at_flow_block_missing_boundary;

        self.parse_block_expression_until(&mut at_missing_body_boundary)
    }

    pub(in crate::parser::expression) fn parse_loop_expression(&mut self) -> LoopExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = LoopExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_loop_keyword(self.expect(SyntaxKind::LoopKeyword));
        builder.push_block_expression(self.parse_flow_block_expression());

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::SyntaxText;

    use super::super::test_support::{
        assert_expression_cases_to_eof_for_test, parse_expression_to_eof_for_test,
    };

    #[test]
    fn pattern_conditions_parse_braces_and_recover_missing_headers() {
        for source in [
            "if let ?value = input {} else if let Data(value) = next() {}",
            "while let { x = value, .. } = next() {} else {}",
            "if input matches Empty {}",
            "if input matches Point { x = 0 } {}",
            "if !(input matches Point { x = 0 }) {}",
            "if (input matches Empty) {}",
            "if Point { x = 1 } matches Point { x = 1 } {}",
            "if (Point { x = 1 }) matches { x = 1 } {}",
            "if let { x } = Point { x = 1 } {}",
            "while let { x } = Point { x = 1 } {}",
            "if let ?value = input && value > 0 {}",
            "while ready && let ?value = next() && value > 0 {} else {}",
            "if let ?first = input && let ?second = first && (second > 0 || ready) {}",
            "if ready || retry && available {}",
            "match input { case Variant { trusted core.target.hardware_fence(MemoryOrder.Acquire,); } }",
            "match input { case Variant { return write<Destination, Failure>(output, content, options); } }",
        ] {
            let (expression, diagnostics) = parse_expression_to_eof_for_test(source);

            assert_eq!(expression.full_text(), source);
            assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
        }

        for source in [
            "if let ?value = {} else {}",
            "while let = input {}",
            "if input matches {} else {}",
            "if let ?value = input || ready {}",
            "if ready || let ?value = input {}",
            "if !(let ?value = input) {}",
            "if (let ?value = input) && ready {}",
        ] {
            let (expression, diagnostics) = parse_expression_to_eof_for_test(source);

            assert_eq!(expression.full_text(), source);
            assert!(!diagnostics.is_empty(), "{source}");
        }
    }

    #[test]
    fn parser_parses_conditional_while_for_and_loop_expressions() {
        let cases = [
            (
                "if ready { yield value; } else if next { yield next; }",
                "if ready { yield value; } else if next { yield next; }",
            ),
            (
                "while ready { continue; } else { break; }",
                "while ready { continue; } else { break; }",
            ),
            (
                "for item in mut items { yield item; } else { yield none; }",
                "for item in mut items { yield item; } else { yield none; }",
            ),
            ("loop { break; }", "loop { break; }"),
        ];

        assert_expression_cases_to_eof_for_test(&cases);
    }
}
