use bray_syntax::{
    AbsenceExpressionSyntax, AccessExpressionSyntax, ExpressionSyntax, GroupedExpressionSyntax,
    LeadingDotVariantExpressionSyntax, LiteralExpressionSyntax, PrimaryExpressionSyntax,
    SyntaxKind, SyntaxToken, TupleExpressionSyntax, TupleExpressionSyntaxBuilder,
    UnitExpressionSyntax,
};

use crate::parser::state::Parser;

use super::grammar::{
    EXPRESSION_START_KINDS, at_access_expression_start, at_literal_expression_start,
    at_primary_hard_boundary,
};

impl Parser {
    pub(in crate::parser::expression) fn parse_primary_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        if at_boundary(self) || at_primary_hard_boundary(self.peek().kind()) {
            return self.missing_primary_expression();
        }

        match self.peek().kind() {
            kind if at_literal_expression_start(kind) => self.parse_literal_primary_expression(),
            SyntaxKind::UnitKeyword => self.parse_unit_primary_expression(),
            SyntaxKind::NoneKeyword => self.parse_absence_primary_expression(),
            kind if at_access_expression_start(kind) => {
                self.parse_access_primary_expression(at_boundary)
            }
            SyntaxKind::OpenParenToken => self.parse_parenthesized_primary_expression(at_boundary),
            SyntaxKind::OpenBracketToken => self.parse_array_primary_expression(at_boundary),
            SyntaxKind::OpenBraceToken if self.should_parse_expected_type_struct_construction() => {
                self.parse_expected_type_struct_construction_primary()
            }
            SyntaxKind::OpenBraceToken => self.parse_block_primary_expression(at_boundary),
            SyntaxKind::DotToken => self.parse_leading_dot_variant_primary_expression(),
            kind if EXPRESSION_START_KINDS.contains(&kind) => {
                self.parse_unsupported_primary_expression(at_boundary)
            }
            _ => self.parse_unknown_primary_expression(at_boundary),
        }
    }

    fn parse_literal_primary_expression(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();

        let mut literal = LiteralExpressionSyntax::builder(self.syntax_source(), start);
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        literal.push_literal_token(self.consume());
        primary.push_literal_expression(literal.build());

        primary.build()
    }

    fn parse_unit_primary_expression(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();

        let mut unit = UnitExpressionSyntax::builder(self.syntax_source(), start);
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        unit.push_unit_keyword(self.expect(SyntaxKind::UnitKeyword));
        primary.push_unit_expression(unit.build());

        primary.build()
    }

    fn parse_absence_primary_expression(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();

        let mut absence = AbsenceExpressionSyntax::builder(self.syntax_source(), start);
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        absence.push_none_keyword(self.expect(SyntaxKind::NoneKeyword));
        primary.push_absence_expression(absence.build());

        primary.build()
    }

    fn parse_access_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        primary.push_access_expression(self.parse_access_expression(at_boundary));

        if self.at(SyntaxKind::OpenBraceToken) {
            primary.push_struct_construction_body(self.parse_struct_construction_body());
        }

        primary.build()
    }

    fn parse_expected_type_struct_construction_primary(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        primary.push_struct_construction_body(self.parse_struct_construction_body());

        primary.build()
    }

    fn parse_block_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        primary.push_block_expression(self.parse_block_expression_until(at_boundary));

        primary.build()
    }

    fn parse_parenthesized_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let open_paren = self.expect(SyntaxKind::OpenParenToken);

        let mut at_expression_boundary =
            |parser: &mut Parser| parser.at_parenthesized_expression_boundary(at_boundary);

        let expression = self.parse_expression_until(&mut at_expression_boundary);

        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        if self.at(SyntaxKind::CommaToken) || self.at_parenthesized_tuple_missing_separator() {
            primary.push_tuple_expression(self.parse_tuple_expression_tail(
                open_paren,
                expression,
                at_boundary,
            ));
        } else {
            primary.push_grouped_expression(
                self.parse_grouped_expression_tail(open_paren, expression),
            );
        }

        primary.build()
    }

    fn parse_grouped_expression_tail(
        &mut self,
        open_paren: SyntaxToken,
        expression: ExpressionSyntax,
    ) -> GroupedExpressionSyntax {
        let start = open_paren.full_range().start();
        let mut builder = GroupedExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(open_paren);
        builder.push_expression(expression);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_tuple_expression_tail(
        &mut self,
        open_paren: SyntaxToken,
        expression: ExpressionSyntax,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TupleExpressionSyntax {
        let start = open_paren.full_range().start();
        let mut builder = TupleExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(open_paren);

        builder.push_expression(expression);
        self.parse_tuple_expression_tail_items(&mut builder, at_boundary);

        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_tuple_expression_tail_items(
        &mut self,
        builder: &mut TupleExpressionSyntaxBuilder,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        loop {
            if self.at(SyntaxKind::CloseParenToken)
                || self.at(SyntaxKind::EndOfFileToken)
                || at_boundary(self)
            {
                break;
            }

            builder.push_separator_token(self.expect(SyntaxKind::CommaToken));

            if self.at(SyntaxKind::CloseParenToken)
                || self.at(SyntaxKind::EndOfFileToken)
                || at_boundary(self)
            {
                break;
            }

            let mut at_tuple_boundary = Parser::at_tuple_expression_element_boundary;

            builder.push_expression(self.parse_expression_until(&mut at_tuple_boundary));
        }
    }

    fn parse_array_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        primary.push_array_expression(self.parse_array_expression(at_boundary));

        primary.build()
    }

    fn parse_leading_dot_variant_primary_expression(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();

        let mut variant = LeadingDotVariantExpressionSyntax::builder(self.syntax_source(), start);
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        variant.push_dot_token(self.expect(SyntaxKind::DotToken));
        variant.push_identifier_token(self.parse_identifier());
        primary.push_leading_dot_variant_expression(variant.build());

        primary.build()
    }

    fn parse_unsupported_primary_expression(
        &mut self,
        _at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        // TODO(parser): Parse remaining primary-expression roots as typed syntax.
        builder.push_token(self.consume());

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

    pub(in crate::parser::expression) fn parse_access_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> AccessExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = AccessExpressionSyntax::builder(self.syntax_source(), start);

        match self.peek().kind() {
            SyntaxKind::IdentifierToken => builder.push_identifier_token(self.parse_identifier()),
            SyntaxKind::SelfValueKeyword => {
                builder.push_self_token(self.expect(SyntaxKind::SelfValueKeyword));
            }
            SyntaxKind::InternalKeyword => {
                builder.push_internal_token(self.expect(SyntaxKind::InternalKeyword));
                builder.push_access_expression(self.parse_access_expression(at_boundary));
            }
            SyntaxKind::OpenParenToken => {
                builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
                builder.push_access_expression(self.parse_access_expression(at_boundary));
                builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));
            }
            _ => {
                self.recover_current_and_until_predicate(&mut builder, |parser| {
                    parser.at_primary_tail_boundary(at_boundary)
                });
            }
        }

        self.parse_access_expression_postfixes(&mut builder, at_boundary);

        builder.build()
    }

    fn parse_access_expression_postfixes(
        &mut self,
        builder: &mut bray_syntax::AccessExpressionSyntaxBuilder,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        loop {
            if at_boundary(self) {
                break;
            }

            match self.peek().kind() {
                SyntaxKind::DotToken => {
                    builder.push_member_access_operation(self.parse_member_access_operation());
                }
                SyntaxKind::OpenBracketToken if !self.should_parse_slice_index_operation() => {
                    builder.push_element_index_operation(
                        self.parse_element_index_operation(at_boundary),
                    );
                }
                _ => break,
            }
        }
    }

    pub(in crate::parser::expression) fn missing_expression(&mut self) -> ExpressionSyntax {
        let primary = self.missing_primary_expression();

        self.primary_to_expression(primary)
    }

    pub(in crate::parser::expression) fn missing_primary_expression(
        &mut self,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{TextRange, TextSize};
    use bray_syntax::{ExpressionSyntax, PrimaryExpressionSyntax, SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::state::Parser;
    use crate::test_support::{diagnostic_kinds, source};

    #[test]
    fn parser_parses_grouped_tuple_array_literals_structs_and_leading_dot_variants() {
        let cases = [
            ("(value);", "(value)", SyntaxKind::GroupedExpression),
            (
                "(value, next,);",
                "(value, next,)",
                SyntaxKind::TupleExpression,
            ),
            (
                "[value, next,];",
                "[value, next,]",
                SyntaxKind::ArrayExpression,
            ),
            ("[value; 3];", "[value; 3]", SyntaxKind::ArrayExpression),
            (
                "Point { x = 1, };",
                "Point { x = 1, }",
                SyntaxKind::StructConstructionBody,
            ),
            (
                "{ x = 1 };",
                "{ x = 1 }",
                SyntaxKind::StructConstructionBody,
            ),
            (
                ".Some(1);",
                ".Some(1)",
                SyntaxKind::LeadingDotVariantExpression,
            ),
            ("true;", "true", SyntaxKind::LiteralExpression),
            ("unit;", "unit", SyntaxKind::UnitExpression),
            ("none;", "none", SyntaxKind::AbsenceExpression),
        ];

        for (source_text, expected_text, expected_kind) in cases {
            let sources = source_store([source_text]);
            let snapshot = source(&sources, 0);

            let mut parser = Parser::new(snapshot);
            let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

            let expression = parser.parse_expression_until(&mut boundary);
            let diagnostics = parser.finish();

            assert_eq!(expression.full_text(), expected_text, "{source_text}");

            assert!(
                expression
                    .tokens()
                    .next()
                    .is_some_and(|token| !token.is_missing()),
                "{source_text}"
            );

            assert!(
                first_primary_expression(&expression).is_some(),
                "{source_text}"
            );

            assert!(
                first_primary_expression(&expression)
                    .is_some_and(|primary| primary_contains_child_kind(&primary, expected_kind)),
                "{source_text}"
            );

            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }

    #[test]
    fn parser_reports_missing_close_for_grouped_expression_at_insertion_point() {
        let sources = source_store(["(value next;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        let token = match expression
            .primary_expression()
            .and_then(|primary| primary.tuple_expressions().next())
            .map(|tuple| tuple.close_paren_token())
        {
            Some(token) => token,
            None => panic!("tuple close paren token should be present"),
        };

        assert_eq!(expression.full_text(), "(value next");
        assert!(token.is_missing());
        assert_eq!(token.range(), TextRange::empty(TextSize::new(11)));

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxExpectedToken
            ]
        );
    }

    fn primary_contains_child_kind(primary: &PrimaryExpressionSyntax, kind: SyntaxKind) -> bool {
        match kind {
            SyntaxKind::AbsenceExpression => primary.absence_expressions().next().is_some(),
            SyntaxKind::ArrayExpression => primary.array_expressions().next().is_some(),
            SyntaxKind::GroupedExpression => primary.grouped_expressions().next().is_some(),
            SyntaxKind::LeadingDotVariantExpression => {
                primary.leading_dot_variant_expressions().next().is_some()
            }
            SyntaxKind::LiteralExpression => primary.literal_expressions().next().is_some(),
            SyntaxKind::StructConstructionBody => {
                primary.struct_construction_bodies().next().is_some()
            }
            SyntaxKind::TupleExpression => primary.tuple_expressions().next().is_some(),
            SyntaxKind::UnitExpression => primary.unit_expressions().next().is_some(),
            _ => false,
        }
    }

    fn first_primary_expression(expression: &ExpressionSyntax) -> Option<PrimaryExpressionSyntax> {
        expression.primary_expression().or_else(|| {
            expression
                .expressions()
                .find_map(|child| first_primary_expression(&child))
        })
    }
}
