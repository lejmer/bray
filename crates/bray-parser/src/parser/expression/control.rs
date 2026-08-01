use bray_syntax::{
    BlockExpressionSyntax, ConditionalElseSyntax, ConditionalExpressionSyntax, ForExpressionSyntax,
    IterationSourceSyntax, LoopExpressionSyntax, SyntaxKind, WhileExpressionSyntax,
};

use crate::parser::state::Parser;

impl Parser {
    pub(in crate::parser::expression) fn parse_conditional_expression(
        &mut self,
    ) -> ConditionalExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ConditionalExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_if_keyword(self.expect(SyntaxKind::IfKeyword));

        let mut at_condition_boundary = Parser::at_expression_before_block_boundary;

        builder.push_expression(self.parse_expression_until(&mut at_condition_boundary));

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

        let mut at_condition_boundary = Parser::at_expression_before_block_boundary;

        builder.push_expression(self.parse_expression_until(&mut at_condition_boundary));

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
    use super::super::test_support::assert_expression_cases_to_eof_for_test;

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
