use bray_syntax::{ExpressionSyntax, SyntaxKind};

use crate::parser::state::Parser;

const PREFIX_RIGHT_BINDING_POWER: u8 = 20;

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
    pub(in crate::parser) fn parse_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        if !self.try_enter_syntax_nesting() {
            return self.recover_expression_nesting_limit(at_boundary);
        }

        let expression = self.parse_assignment_expression_until(at_boundary);

        self.leave_syntax_nesting();

        expression
    }

    pub(in crate::parser) fn parse_non_assignment_expression_until(
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

        if at_boundary(self) || !at_assignment_operator(self.peek().kind()) {
            return left;
        }

        let start = left.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(left);
        builder.push_operator_token(self.consume());
        builder.push_expression(self.parse_expression_until(at_boundary));

        builder.build()
    }

    fn parse_expression_with_min_binding_power_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
        min_binding_power: u8,
    ) -> ExpressionSyntax {
        if !self.try_enter_syntax_nesting() {
            return self.recover_expression_nesting_limit(at_boundary);
        }

        let mut expression = self.parse_prefix_expression_until(at_boundary);
        let mut non_associative_binding_power = None;

        loop {
            if at_boundary(self) {
                break;
            }

            let Some(operator) = infix_operator(self.peek().kind()) else {
                break;
            };

            if non_associative_binding_power == Some(operator.left_binding_power) {
                break;
            }

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
                non_associative_binding_power = Some(operator.left_binding_power);
            }
        }

        self.leave_syntax_nesting();

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
}

pub(in crate::parser::expression) fn at_infix_operator(kind: SyntaxKind) -> bool {
    infix_operator(kind).is_some()
}

fn at_assignment_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::EqualsToken
            | SyntaxKind::PlusEqualsToken
            | SyntaxKind::MinusEqualsToken
            | SyntaxKind::StarEqualsToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::PercentEqualsToken
            | SyntaxKind::AtEqualsToken
            | SyntaxKind::AmpersandEqualsToken
            | SyntaxKind::PipeEqualsToken
            | SyntaxKind::CaretEqualsToken
            | SyntaxKind::LessLessEqualsToken
            | SyntaxKind::GreaterGreaterEqualsToken
            | SyntaxKind::StarStarEqualsToken
    )
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

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::state::Parser;
    use crate::test_support::{diagnostic_kinds, source};

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
    fn parser_preserves_every_compound_assignment_operator() {
        let cases = [
            ("+=", SyntaxKind::PlusEqualsToken),
            ("-=", SyntaxKind::MinusEqualsToken),
            ("*=", SyntaxKind::StarEqualsToken),
            ("/=", SyntaxKind::SlashEqualsToken),
            ("%=", SyntaxKind::PercentEqualsToken),
            ("@=", SyntaxKind::AtEqualsToken),
            ("&=", SyntaxKind::AmpersandEqualsToken),
            ("|=", SyntaxKind::PipeEqualsToken),
            ("^=", SyntaxKind::CaretEqualsToken),
            ("<<=", SyntaxKind::LessLessEqualsToken),
            (">>=", SyntaxKind::GreaterGreaterEqualsToken),
            ("**=", SyntaxKind::StarStarEqualsToken),
        ];

        for (spelling, kind) in cases {
            let source_text = format!("left {spelling} right;");
            let sources = source_store([source_text.as_str()]);
            let snapshot = source(&sources, 0);
            let mut parser = Parser::new(snapshot);
            let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

            let expression = parser.parse_expression_until(&mut boundary);
            let diagnostics = parser.finish();

            assert_eq!(expression.full_text(), format!("left {spelling} right"));

            assert_eq!(
                expression.operator_token().map(|token| token.kind()),
                Some(kind)
            );

            assert!(diagnostics.is_empty(), "{spelling}: {diagnostics:?}");
        }
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
    fn parser_continues_from_comparison_into_logical_operator() {
        let sources = source_store(["left < middle && middle < right;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);

        assert_eq!(expression.full_text(), "left < middle && middle < right");

        assert_eq!(
            expression.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::AmpersandAmpersandToken)
        );

        assert_eq!(parser.peek().kind(), SyntaxKind::SemicolonToken);

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
    fn flat_operator_chains_reconstruct_without_recursive_green_walks() {
        let expression_text = format!("{}value", "value + ".repeat(4_096));
        let sources = source_store([format!("{expression_text};")]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);

        assert_eq!(expression.full_text(), expression_text);
        assert_eq!(parser.peek().kind(), SyntaxKind::SemicolonToken);

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }
}
