use bray_syntax::{
    MatchArmSyntax, MatchBodySyntax, MatchBodySyntaxBuilder, MatchExpressionSyntax,
    MatchSubjectSyntax, SyntaxKind,
};

use crate::parser::state::Parser;

impl Parser {
    pub(in crate::parser::expression) fn parse_match_expression(
        &mut self,
    ) -> MatchExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = MatchExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_match_keyword(self.expect(SyntaxKind::MatchKeyword));
        builder.push_match_subject(self.parse_match_subject());
        builder.push_match_body(self.parse_match_body());

        builder.build()
    }

    fn parse_match_subject(&mut self) -> MatchSubjectSyntax {
        let start = self.peek().full_range().start();
        let mut builder = MatchSubjectSyntax::builder(self.syntax_source(), start);

        if self.at(SyntaxKind::ConsumeKeyword) {
            builder.push_consume_keyword(self.expect(SyntaxKind::ConsumeKeyword));
        }

        let mut at_subject_boundary = Parser::at_expression_before_block_boundary;

        builder.push_expression(self.parse_expression_until(&mut at_subject_boundary));

        self.recover_until_predicate(
            &mut builder,
            Parser::at_expression_before_block_recovery_boundary,
        );

        builder.build()
    }

    fn parse_match_body(&mut self) -> MatchBodySyntax {
        let start = self.peek().full_range().start();
        let mut builder = MatchBodySyntax::builder(self.syntax_source(), start);

        self.parse_braced_body_contents(
            &mut builder,
            Parser::at_match_body_missing_boundary,
            |parser, builder| parser.parse_match_arms(builder),
        );

        builder.build()
    }

    fn parse_match_arms(&mut self, builder: &mut MatchBodySyntaxBuilder) {
        let mut parsed_any = false;

        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFileToken) {
            if self.at(SyntaxKind::CaseKeyword) {
                parsed_any = true;
                builder.push_match_arm(self.parse_match_arm());
            } else {
                self.recover_current_and_until_predicate(builder, Parser::at_match_arm_boundary);
            }
        }

        if !parsed_any {
            builder.push_match_arm(self.parse_match_arm());
        }
    }

    fn parse_match_arm(&mut self) -> MatchArmSyntax {
        let start = self.peek().full_range().start();
        let mut builder = MatchArmSyntax::builder(self.syntax_source(), start);

        builder.push_case_keyword(self.expect(SyntaxKind::CaseKeyword));

        let mut at_pattern_boundary = Parser::at_match_arm_pattern_boundary;

        builder.push_case_pattern(self.parse_case_pattern_until(&mut at_pattern_boundary));

        if self.at(SyntaxKind::WhenKeyword) {
            builder.push_when_keyword(self.expect(SyntaxKind::WhenKeyword));

            let mut at_guard_boundary = Parser::at_match_arm_guard_boundary;

            builder.push_expression(self.parse_expression_until(&mut at_guard_boundary));
            self.recover_until_predicate(&mut builder, Parser::at_match_arm_guard_boundary);
        }

        builder.push_block_expression(self.parse_flow_block_expression());

        builder.build()
    }

    fn at_match_body_missing_boundary(&mut self) -> bool {
        self.at(SyntaxKind::EndOfFileToken) || self.at(SyntaxKind::SemicolonToken)
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{
        ExpressionSyntax, MatchArmSyntax, MatchBodySyntax, MatchExpressionSyntax, SyntaxKind,
        SyntaxText,
    };
    use bray_testing::test_source_store as source_store;

    use crate::parser::state::Parser;
    use crate::test_support::{diagnostic_kinds, source};

    #[test]
    fn parser_parses_match_subject_arms_and_guards() {
        let sources = source_store([
            "match consume value { case Some(item) when item > 0 { yield item; } case none { yield 0; } };",
        ]);

        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let match_expression = first_match_expression(&expression);

        assert_eq!(
            expression.full_text(),
            "match consume value { case Some(item) when item > 0 { yield item; } case none { yield 0; } }"
        );

        assert_eq!(match_expression.match_body().match_arms().count(), 2);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_recovers_bad_match_body_tokens_before_later_arms() {
        let sources = source_store(["match value { $ case item { yield item; } };"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let match_expression = first_match_expression(&expression);

        let skipped = match_expression
            .match_body()
            .skipped_syntax()
            .collect::<Vec<_>>();

        assert_eq!(match_expression.match_body().match_arms().count(), 1);

        assert!(
            skipped
                .iter()
                .any(|skipped_syntax| skipped_syntax.full_text().contains('$')),
            "{skipped:?}"
        );

        let diagnostic_kinds = diagnostic_kinds(&diagnostics);

        assert!(diagnostic_kinds.contains(&DiagnosticKind::LexicalInvalidCharacter));
    }

    #[test]
    fn parser_reports_missing_match_arm_for_empty_match_bodies() {
        let sources = source_store(["match value { };"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        let match_expression = first_match_expression(&expression);
        let match_body = match_expression.match_body();
        let match_arm = first_match_arm(&match_body);

        assert_eq!(expression.full_text(), "match value { }");
        assert!(!match_body.close_brace_token().is_missing());
        assert!(match_arm.case_keyword().is_missing());
        assert!(match_arm.block_expression().open_brace_token().is_missing());

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxExpectedToken
            ]
        );
    }

    #[test]
    fn parser_reports_missing_match_arm_body_without_consuming_match_body_close() {
        let sources = source_store(["match value { case item };"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        let match_expression = first_match_expression(&expression);
        let match_body = match_expression.match_body();
        let match_arm = first_match_arm(&match_body);

        let block_expression = match_arm.block_expression();

        assert_eq!(expression.full_text(), "match value { case item }");
        assert!(!match_body.close_brace_token().is_missing());
        assert!(block_expression.open_brace_token().is_missing());
        assert!(block_expression.close_brace_token().is_missing());

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    fn first_match_expression(expression: &ExpressionSyntax) -> MatchExpressionSyntax {
        let primary = match expression.primary_expression() {
            Some(primary) => primary,
            None => panic!("expected primary"),
        };

        match primary.match_expressions().next() {
            Some(match_expression) => match_expression,
            None => panic!("expected match expression"),
        }
    }

    fn first_match_arm(match_body: &MatchBodySyntax) -> MatchArmSyntax {
        match match_body.match_arms().next() {
            Some(match_arm) => match_arm,
            None => panic!("expected match arm"),
        }
    }
}
