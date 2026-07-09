use bray_syntax::{
    AsyncBlockExpressionSyntax, BreakExpressionSyntax, ContinueExpressionSyntax,
    PanicExpressionSyntax, ReturnExpressionSyntax, SpawnExpressionSyntax, SyntaxKind,
    WithExpressionSyntax, YieldExpressionSyntax,
};

use crate::parser::state::Parser;

impl Parser {
    pub(in crate::parser::expression) fn parse_with_expression(&mut self) -> WithExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = WithExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_with_keyword(self.expect(SyntaxKind::WithKeyword));

        let mut at_pattern_boundary = Parser::at_with_pattern_boundary;

        builder.push_irrefutable_pattern(
            self.parse_irrefutable_pattern_until(&mut at_pattern_boundary),
        );

        if self.at(SyntaxKind::ColonToken) {
            let mut at_type_boundary = Parser::at_with_type_boundary;

            builder.push_type_annotation(self.parse_type_annotation_until(&mut at_type_boundary));
        }

        builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));

        let mut at_initializer_boundary = Parser::at_expression_before_block_boundary;

        builder.push_expression(self.parse_expression_until(&mut at_initializer_boundary));

        self.recover_until_predicate(
            &mut builder,
            Parser::at_expression_before_block_recovery_boundary,
        );

        builder.push_block_expression(self.parse_flow_block_expression());

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_async_block_expression(
        &mut self,
    ) -> AsyncBlockExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = AsyncBlockExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_async_keyword(self.expect(SyntaxKind::AsyncKeyword));
        builder.push_block_expression(self.parse_flow_block_expression());

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_spawn_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> SpawnExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SpawnExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_spawn_keyword(self.expect(SyntaxKind::SpawnKeyword));

        match self.peek().kind() {
            SyntaxKind::DetachedKeyword => {
                builder.push_detached_keyword(self.expect(SyntaxKind::DetachedKeyword));
                builder.push_expression(self.parse_expression_until(at_boundary));
            }
            SyntaxKind::ThreadKeyword => {
                builder.push_thread_keyword(self.expect(SyntaxKind::ThreadKeyword));

                let mut at_callee_boundary = |parser: &mut Parser| {
                    parser.at(SyntaxKind::OpenParenToken) || at_boundary(parser)
                };

                builder
                    .push_access_expression(self.parse_access_expression(&mut at_callee_boundary));

                builder.push_argument_list(self.parse_argument_list());
            }
            _ => {
                builder.push_expression(self.parse_expression_until(at_boundary));
            }
        }

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_yield_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> YieldExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = YieldExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_yield_keyword(self.expect(SyntaxKind::YieldKeyword));

        if !self.at_expression_operand_boundary(at_boundary) {
            builder.push_expression(self.parse_expression_until(at_boundary));
        }

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_return_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ReturnExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ReturnExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_return_keyword(self.expect(SyntaxKind::ReturnKeyword));

        if !self.at_expression_operand_boundary(at_boundary) {
            builder.push_expression(self.parse_expression_until(at_boundary));
        }

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_panic_expression(
        &mut self,
    ) -> PanicExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PanicExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_panic_keyword(self.expect(SyntaxKind::PanicKeyword));
        builder.push_argument_list(self.parse_argument_list());

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_break_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> BreakExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = BreakExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_break_keyword(self.expect(SyntaxKind::BreakKeyword));

        if !self.at_expression_operand_boundary(at_boundary) {
            builder.push_expression(self.parse_expression_until(at_boundary));
        }

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_continue_expression(
        &mut self,
    ) -> ContinueExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ContinueExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_continue_keyword(self.expect(SyntaxKind::ContinueKeyword));

        builder.build()
    }

    fn at_with_pattern_boundary(&mut self) -> bool {
        self.at(SyntaxKind::ColonToken)
            || self.at(SyntaxKind::EqualsToken)
            || self.at_expression_before_block_boundary()
    }

    fn at_with_type_boundary(&mut self) -> bool {
        self.at(SyntaxKind::EqualsToken) || self.at_expression_before_block_boundary()
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::assert_expression_cases_until_semicolon_for_test;

    #[test]
    fn parser_parses_block_shaped_flow_expressions() {
        let cases = [
            (
                "with value: Item = source { yield value; };",
                "with value: Item = source { yield value; }",
            ),
            ("async { return value; };", "async { return value; }"),
            ("spawn worker();", "spawn worker()"),
            ("spawn detached worker();", "spawn detached worker()"),
            ("spawn thread worker(value);", "spawn thread worker(value)"),
        ];

        assert_expression_cases_until_semicolon_for_test(&cases);
    }

    #[test]
    fn parser_parses_optional_operand_flow_expressions() {
        let cases = [
            ("yield;", "yield"),
            ("yield value;", "yield value"),
            ("return;", "return"),
            ("return value;", "return value"),
            ("break;", "break"),
            ("break value;", "break value"),
            ("continue;", "continue"),
            ("panic(\"failed\");", "panic(\"failed\")"),
        ];

        assert_expression_cases_until_semicolon_for_test(&cases);
    }
}
