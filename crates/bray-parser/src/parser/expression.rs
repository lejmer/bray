use bray_syntax::{
    ExpressionSyntax, PrimaryExpressionSyntax, PrimaryExpressionSyntaxBuilder, SyntaxKind,
    SyntaxToken,
};

use super::state::Parser;

const PREFIX_RIGHT_BINDING_POWER: u8 = 20;

pub(super) const EXPRESSION_START_KINDS: [SyntaxKind; 43] = [
    SyntaxKind::AllKeyword,
    SyntaxKind::AmpersandToken,
    SyntaxKind::AnyKeyword,
    SyntaxKind::AssertKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::AwaitKeyword,
    SyntaxKind::BangToken,
    SyntaxKind::BinaryIntegerLiteralToken,
    SyntaxKind::BreakKeyword,
    SyntaxKind::CatchKeyword,
    SyntaxKind::CharacterLiteralToken,
    SyntaxKind::ContinueKeyword,
    SyntaxKind::DecimalIntegerLiteralToken,
    SyntaxKind::DotToken,
    SyntaxKind::FalseKeyword,
    SyntaxKind::ForKeyword,
    SyntaxKind::HexadecimalIntegerLiteralToken,
    SyntaxKind::IdentifierToken,
    SyntaxKind::IfKeyword,
    SyntaxKind::ImaginaryLiteralToken,
    SyntaxKind::LambdaKeyword,
    SyntaxKind::LoopKeyword,
    SyntaxKind::MatchKeyword,
    SyntaxKind::MinusToken,
    SyntaxKind::NoneKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::OpenBracketToken,
    SyntaxKind::OpenParenToken,
    SyntaxKind::PanicKeyword,
    SyntaxKind::RealLiteralToken,
    SyntaxKind::ReturnKeyword,
    SyntaxKind::SelfValueKeyword,
    SyntaxKind::SpawnKeyword,
    SyntaxKind::StringLiteralToken,
    SyntaxKind::TildeToken,
    SyntaxKind::TrueKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::TryKeyword,
    SyntaxKind::TupleElementIndexToken,
    SyntaxKind::UnitKeyword,
    SyntaxKind::WhileKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::YieldKeyword,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperatorAssociativity {
    Left,
    Right,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InfixOperator {
    left_binding_power: u8,
    right_binding_power: u8,
    associativity: OperatorAssociativity,
}

impl Parser {
    pub(super) fn parse_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        self.parse_assignment_expression_until(at_boundary)
    }

    pub(super) fn parse_non_assignment_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        self.parse_expression_with_min_binding_power_until(at_boundary, 0)
    }

    fn parse_assignment_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let left = self.parse_non_assignment_expression_until(at_boundary);

        if at_boundary(self) || !self.at(SyntaxKind::EqualsToken) {
            return left;
        }

        let start = left.full_range().start();

        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(left);
        builder.push_operator_token(self.expect(SyntaxKind::EqualsToken));
        builder.push_expression(self.parse_expression_until(at_boundary));

        builder.build()
    }

    fn parse_expression_with_min_binding_power_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
        min_binding_power: u8,
    ) -> ExpressionSyntax {
        let mut expression = self.parse_prefix_expression_until(at_boundary);

        loop {
            if at_boundary(self) {
                break;
            }

            let Some(operator) = infix_operator(self.peek().kind()) else {
                break;
            };

            if operator.left_binding_power < min_binding_power {
                break;
            }

            let start = expression.full_range().start();

            let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

            builder.push_expression(expression);
            builder.push_operator_token(self.consume());
            builder.push_expression(self.parse_expression_with_min_binding_power_until(
                at_boundary,
                operator.right_binding_power,
            ));

            expression = builder.build();

            if operator.associativity == OperatorAssociativity::None {
                break;
            }
        }

        expression
    }

    fn parse_prefix_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        if at_boundary(self) {
            return self.missing_expression();
        }

        if !at_unary_operator(self.peek().kind()) {
            return self.parse_postfix_expression_until(at_boundary);
        }

        let start = self.peek().full_range().start();

        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_operator_token(self.consume());
        builder.push_expression(self.parse_expression_with_min_binding_power_until(
            at_boundary,
            PREFIX_RIGHT_BINDING_POWER,
        ));

        builder.build()
    }

    fn parse_postfix_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let primary = self.parse_primary_expression_until(at_boundary);
        let start = primary.full_range().start();

        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_primary_expression(primary);

        builder.build()
    }

    fn parse_primary_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        if at_boundary(self) || at_primary_hard_boundary(self.peek().kind()) {
            return self.missing_primary_expression();
        }

        match self.peek().kind() {
            kind if at_simple_primary_expression_start(kind) => {
                self.parse_simple_primary_expression(at_boundary)
            }
            SyntaxKind::OpenParenToken => self.parse_parenthesized_primary_expression(at_boundary),
            SyntaxKind::OpenBracketToken => self.parse_opaque_delimited_primary_expression(
                SyntaxKind::OpenBracketToken,
                SyntaxKind::CloseBracketToken,
                at_boundary,
            ),
            SyntaxKind::OpenBraceToken => self.parse_block_primary_expression(at_boundary),
            _ => self.parse_unknown_primary_expression(at_boundary),
        }
    }

    fn parse_simple_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_token(self.consume());
        self.recover_opaque_primary_tail_for_now(&mut builder, at_boundary);

        builder.build()
    }

    fn parse_block_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_block_expression(self.parse_block_expression_until(at_boundary));
        self.recover_opaque_primary_tail_for_now(&mut builder, at_boundary);

        builder.build()
    }

    fn parse_parenthesized_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();

        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_token(self.expect(SyntaxKind::OpenParenToken));
        builder.push_expression(self.parse_expression_until(&mut |parser| {
            parser.at_parenthesized_primary_boundary(at_boundary)
        }));

        if self.at(SyntaxKind::CommaToken) {
            self.recover_until_predicate(&mut builder, |parser| {
                parser.at(SyntaxKind::CloseParenToken) || at_boundary(parser)
            });
        }

        builder.push_token(self.expect(SyntaxKind::CloseParenToken));
        self.recover_opaque_primary_tail_for_now(&mut builder, at_boundary);

        builder.build()
    }

    fn parse_opaque_delimited_primary_expression(
        &mut self,
        open_kind: SyntaxKind,
        close_kind: SyntaxKind,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();

        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_token(self.expect(open_kind));

        self.recover_opaque_delimited_primary_contents(
            &mut builder,
            open_kind,
            close_kind,
            at_boundary,
        );

        builder.push_token(self.expect(close_kind));

        self.recover_opaque_primary_tail_for_now(&mut builder, at_boundary);

        builder.build()
    }

    fn parse_unknown_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        self.recover_current_and_until_predicate(&mut builder, |parser| {
            parser.at_primary_tail_boundary(at_boundary)
        });

        builder.build()
    }

    fn missing_expression(&mut self) -> ExpressionSyntax {
        let primary = self.missing_primary_expression();
        let start = primary.full_range().start();

        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_primary_expression(primary);

        builder.build()
    }

    fn missing_primary_expression(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    fn recover_opaque_delimited_primary_contents(
        &mut self,
        builder: &mut PrimaryExpressionSyntaxBuilder,
        open_kind: SyntaxKind,
        close_kind: SyntaxKind,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        let mut skipped_tokens = Vec::new();
        let mut depth = 0usize;

        while !(self.at(SyntaxKind::EndOfFileToken)
            || at_boundary(self)
            || depth == 0 && self.at(close_kind))
        {
            let token = self.consume();

            match token.kind() {
                kind if kind == open_kind => depth += 1,
                kind if kind == close_kind => depth = depth.saturating_sub(1),
                _ => {}
            }

            skipped_tokens.push(token);
        }

        self.record_skipped_syntax_for_tokens(&skipped_tokens);
        builder.push_skipped_tokens(skipped_tokens);
    }

    fn recover_opaque_primary_tail_for_now(
        &mut self,
        builder: &mut PrimaryExpressionSyntaxBuilder,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        let mut skipped_tokens = Vec::new();

        while !self.at_primary_tail_boundary(at_boundary) {
            self.consume_opaque_primary_tail_token(&mut skipped_tokens);
        }

        self.record_skipped_syntax_for_tokens(&skipped_tokens);
        builder.push_skipped_tokens(skipped_tokens);
    }

    fn consume_opaque_primary_tail_token(&mut self, skipped_tokens: &mut Vec<SyntaxToken>) {
        let token = self.consume();
        let token_kind = token.kind();
        let close_kind = matching_close_kind(token_kind);

        skipped_tokens.push(token);

        let Some(close_kind) = close_kind else {
            return;
        };

        self.consume_opaque_primary_tail_delimiter(token_kind, close_kind, skipped_tokens);
    }

    fn consume_opaque_primary_tail_delimiter(
        &mut self,
        open_kind: SyntaxKind,
        close_kind: SyntaxKind,
        skipped_tokens: &mut Vec<SyntaxToken>,
    ) {
        let mut depth = 1usize;

        while depth > 0 && !self.at(SyntaxKind::EndOfFileToken) {
            let token = self.consume();

            match token.kind() {
                kind if kind == open_kind => depth += 1,
                kind if kind == close_kind => depth = depth.saturating_sub(1),
                _ => {}
            }

            skipped_tokens.push(token);
        }
    }

    fn at_parenthesized_primary_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(SyntaxKind::CloseParenToken) || self.at(SyntaxKind::CommaToken) || at_boundary(self)
    }

    fn at_primary_tail_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        let kind = self.peek().kind();

        at_boundary(self)
            || at_primary_hard_boundary(kind)
            || kind == SyntaxKind::EqualsToken
            || infix_operator(kind).is_some()
    }
}

fn infix_operator(kind: SyntaxKind) -> Option<InfixOperator> {
    let (left_binding_power, right_binding_power, associativity) = match kind {
        SyntaxKind::PipePipeToken => (1, 2, OperatorAssociativity::Left),
        SyntaxKind::AmpersandAmpersandToken => (3, 4, OperatorAssociativity::Left),
        SyntaxKind::EqualsEqualsToken
        | SyntaxKind::BangEqualsToken
        | SyntaxKind::LessToken
        | SyntaxKind::LessEqualsToken
        | SyntaxKind::GreaterToken
        | SyntaxKind::GreaterEqualsToken => (5, 6, OperatorAssociativity::None),
        SyntaxKind::PipeToken => (7, 8, OperatorAssociativity::Left),
        SyntaxKind::CaretToken => (9, 10, OperatorAssociativity::Left),
        SyntaxKind::AmpersandToken => (11, 12, OperatorAssociativity::Left),
        SyntaxKind::LessLessToken | SyntaxKind::GreaterGreaterToken => {
            (13, 14, OperatorAssociativity::Left)
        }
        SyntaxKind::PlusToken | SyntaxKind::MinusToken => (15, 16, OperatorAssociativity::Left),
        SyntaxKind::StarToken
        | SyntaxKind::SlashToken
        | SyntaxKind::PercentToken
        | SyntaxKind::AtToken => (17, 18, OperatorAssociativity::Left),
        SyntaxKind::StarStarToken => (21, 20, OperatorAssociativity::Right),
        _ => return None,
    };

    Some(InfixOperator {
        left_binding_power,
        right_binding_power,
        associativity,
    })
}

fn at_unary_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::MinusToken | SyntaxKind::TildeToken | SyntaxKind::BangToken
    )
}

fn at_simple_primary_expression_start(kind: SyntaxKind) -> bool {
    EXPRESSION_START_KINDS.contains(&kind)
        && !at_unary_operator(kind)
        && !matches!(
            kind,
            SyntaxKind::OpenParenToken | SyntaxKind::OpenBracketToken | SyntaxKind::OpenBraceToken
        )
}

fn at_primary_hard_boundary(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CommaToken
            | SyntaxKind::CloseParenToken
            | SyntaxKind::CloseBracketToken
            | SyntaxKind::CloseBraceToken
            | SyntaxKind::SemicolonToken
            | SyntaxKind::EndOfFileToken
    )
}

fn matching_close_kind(open_kind: SyntaxKind) -> Option<SyntaxKind> {
    match open_kind {
        SyntaxKind::OpenParenToken => Some(SyntaxKind::CloseParenToken),
        SyntaxKind::OpenBracketToken => Some(SyntaxKind::CloseBracketToken),
        SyntaxKind::OpenBraceToken => Some(SyntaxKind::CloseBraceToken),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::test_support::{diagnostic_kinds, source};

    use super::super::state::Parser;

    #[test]
    fn parser_parses_expression_precedence_and_associativity() {
        let sources = source_store(["left + right * -tail ** value;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let children = expression.expressions().collect::<Vec<_>>();

        let [left, right] = children.as_slice() else {
            panic!("expected binary expression children: {children:?}");
        };

        assert_eq!(expression.full_text(), "left + right * -tail ** value");

        assert_eq!(
            expression.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::PlusToken)
        );

        assert!(left.primary_expression().is_some());

        assert_eq!(
            right.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::StarToken)
        );

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_assignment_as_right_associative() {
        let sources = source_store(["left = middle = right;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let children = expression.expressions().collect::<Vec<_>>();

        let [_left, right] = children.as_slice() else {
            panic!("expected assignment expression children: {children:?}");
        };

        assert_eq!(expression.full_text(), "left = middle = right");

        assert_eq!(
            expression.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::EqualsToken)
        );

        assert_eq!(
            right.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::EqualsToken)
        );

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_non_assignment_expression_stops_before_assignment() {
        let sources = source_store(["left = right;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_non_assignment_expression_until(&mut boundary);

        assert_eq!(expression.full_text(), "left ");
        assert_eq!(parser.peek().kind(), SyntaxKind::EqualsToken);

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_leaves_second_comparison_for_recovery() {
        let sources = source_store(["left < middle < right;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);

        assert_eq!(expression.full_text(), "left < middle ");
        assert_eq!(parser.peek().kind(), SyntaxKind::LessToken);

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_inserts_missing_expression_operand_at_boundary() {
        let sources = source_store(["value + ;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let children = expression.expressions().collect::<Vec<_>>();

        let [_left, right] = children.as_slice() else {
            panic!("expected binary expression children: {children:?}");
        };

        let primary = match right.primary_expression() {
            Some(primary) => primary,
            None => panic!("expected missing primary expression"),
        };

        let token = match primary.primary_token() {
            Some(token) => token,
            None => panic!("expected missing primary token"),
        };

        assert_eq!(expression.full_text(), "value + ");
        assert!(token.is_missing());

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_keeps_opaque_primary_tails_inside_primary_expression() {
        let sources = source_store(["target.call(1, 2) + next;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let children = expression.expressions().collect::<Vec<_>>();

        let [left, _right] = children.as_slice() else {
            panic!("expected binary expression children: {children:?}");
        };

        let primary = match left.primary_expression() {
            Some(primary) => primary,
            None => panic!("expected primary expression"),
        };

        let skipped_syntax = primary.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected skipped primary tail: {skipped_syntax:?}");
        };

        assert_eq!(expression.full_text(), "target.call(1, 2) + next");
        assert_eq!(skipped.full_text(), ".call(1, 2) ");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }
}
