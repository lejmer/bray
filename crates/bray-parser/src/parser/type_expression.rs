use bray_syntax::{
    PathSyntax, SyntaxKind, SyntaxToken, TypeExpressionSyntax, TypeExpressionSyntaxBuilder,
};

use super::state::Parser;

const TYPE_ARGUMENT_HARD_BOUNDARY_KINDS: [SyntaxKind; 9] = [
    SyntaxKind::CloseParenToken,
    SyntaxKind::CloseBracketToken,
    SyntaxKind::ArrowToken,
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
    SyntaxKind::FuncKeyword,
];

const ARRAY_SIZE_BOUNDARY_KINDS: [SyntaxKind; 2] =
    [SyntaxKind::CloseBracketToken, SyntaxKind::EndOfFileToken];

impl Parser {
    pub(super) fn parse_type_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        self.parse_prefix_type_expression(at_boundary)
    }

    pub(super) fn missing_type_expression(&mut self) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut path = PathSyntax::builder(self.syntax_source());
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        path.push_identifier_token(SyntaxToken::missing(SyntaxKind::IdentifierToken, start));
        builder.push_path(path.build());

        builder.build()
    }

    fn parse_prefix_type_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        match self.peek().kind() {
            SyntaxKind::AmpersandToken => self.parse_borrow_type_expression(at_boundary),
            SyntaxKind::BoxKeyword => self.parse_box_type_expression(at_boundary),
            SyntaxKind::FuncKeyword => self.parse_callable_type_expression(at_boundary),
            _ => self.parse_postfix_type_expression(at_boundary),
        }
    }

    fn parse_borrow_type_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_ampersand_token(self.expect(SyntaxKind::AmpersandToken));

        if self.at(SyntaxKind::MutKeyword) {
            builder.push_mut_token(self.expect(SyntaxKind::MutKeyword));
        }

        builder.push_type_expression(self.parse_type_expression_until(at_boundary));

        builder.build()
    }

    fn parse_box_type_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_box_keyword(self.expect(SyntaxKind::BoxKeyword));

        if self.at(SyntaxKind::LessToken) {
            // TODO(parser): Parse type form argument lists once generic syntax is implemented.
            self.recover_type_argument_list(&mut builder);
        }

        builder.push_type_expression(self.parse_type_expression_until(at_boundary));

        builder.build()
    }

    fn parse_callable_type_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_func_keyword(self.expect(SyntaxKind::FuncKeyword));
        builder.push_parameter_list(self.parse_parameter_list());

        if self.at(SyntaxKind::ArrowToken) {
            builder.push_callable_result_clause(self.parse_callable_result_clause_until(
                |parser| parser.at_callable_type_tail_boundary(at_boundary),
            ));
        }

        if self.at_callable_contract_clause_start() {
            // TODO(parser): Parse callable contract clauses once expressions are implemented.
            self.recover_current_and_until_predicate(&mut builder, |parser| at_boundary(parser));
        }

        builder.build()
    }

    fn parse_postfix_type_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        let mut expression = self.parse_type_primary_expression(at_boundary);

        loop {
            if !self.at(SyntaxKind::LessToken) {
                break;
            }

            // TODO(parser): Parse generic argument lists once generic syntax is implemented.
            let start = expression.full_range().start();
            let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

            builder.push_type_expression(expression);
            self.recover_type_argument_list(&mut builder);

            expression = builder.build();
        }

        expression
    }

    fn parse_type_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        match self.peek().kind() {
            SyntaxKind::SelfTypeKeyword => self.parse_self_type_expression(),
            SyntaxKind::OpenParenToken => self.parse_parenthesized_type_expression(at_boundary),
            SyntaxKind::OpenBracketToken => self.parse_bracketed_type_expression(at_boundary),
            kind if self.at_type_expression_boundary_kind(kind) || at_boundary(self) => {
                self.parse_path_type_expression()
            }
            SyntaxKind::IdentifierToken => self.parse_path_type_expression(),
            _ => self.parse_unknown_type_expression(at_boundary),
        }
    }

    fn parse_self_type_expression(&mut self) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_self_keyword(self.expect(SyntaxKind::SelfTypeKeyword));

        builder.build()
    }

    fn parse_path_type_expression(&mut self) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_path(self.parse_path());

        builder.build()
    }

    fn parse_parenthesized_type_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));

        builder.push_type_expression(
            self.parse_type_expression_until(&mut |parser| {
                parser.at_type_tuple_boundary(at_boundary)
            }),
        );

        while self.at(SyntaxKind::CommaToken) {
            builder.push_separator_token(self.expect(SyntaxKind::CommaToken));

            if self.at(SyntaxKind::CloseParenToken) {
                break;
            }

            builder.push_type_expression(self.parse_type_expression_until(&mut |parser| {
                parser.at_type_tuple_boundary(at_boundary)
            }));
        }

        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_bracketed_type_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_open_bracket_token(self.expect(SyntaxKind::OpenBracketToken));

        builder.push_type_expression(self.parse_type_expression_until(&mut |parser| {
            parser.at_bracketed_type_boundary(at_boundary)
        }));

        if self.at(SyntaxKind::SemicolonToken) {
            builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));
            // TODO(parser): Parse array constant expressions once constant-expression parsing is implemented.
            self.recover_until(&mut builder, &ARRAY_SIZE_BOUNDARY_KINDS);
        }

        builder.push_close_bracket_token(self.expect(SyntaxKind::CloseBracketToken));

        builder.build()
    }

    fn parse_unknown_type_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        self.recover_current_and_until_predicate(&mut builder, |parser| at_boundary(parser));

        builder.build()
    }

    fn recover_type_argument_list(&mut self, builder: &mut TypeExpressionSyntaxBuilder) {
        let mut skipped_tokens = Vec::new();

        if !self.at(SyntaxKind::EndOfFileToken) {
            skipped_tokens.push(self.consume());
        }

        while !self.at(SyntaxKind::EndOfFileToken)
            && !self.at(SyntaxKind::GreaterToken)
            && !self.at_any(&TYPE_ARGUMENT_HARD_BOUNDARY_KINDS)
        {
            skipped_tokens.push(self.consume());
        }

        if self.at(SyntaxKind::GreaterToken) {
            skipped_tokens.push(self.consume());
        }

        self.record_skipped_syntax_for_tokens(&skipped_tokens);
        builder.push_skipped_tokens(skipped_tokens);
    }

    fn at_type_tuple_boundary(&mut self, at_boundary: &mut dyn FnMut(&mut Parser) -> bool) -> bool {
        self.at(SyntaxKind::CommaToken) || self.at(SyntaxKind::CloseParenToken) || at_boundary(self)
    }

    fn at_bracketed_type_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(SyntaxKind::SemicolonToken)
            || self.at(SyntaxKind::CloseBracketToken)
            || at_boundary(self)
    }

    fn at_callable_type_tail_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at_callable_contract_clause_start() || at_boundary(self)
    }

    fn at_callable_contract_clause_start(&mut self) -> bool {
        self.at(SyntaxKind::RequiresKeyword)
            || self.at(SyntaxKind::EnsuresKeyword)
            || self.at(SyntaxKind::WithKeyword)
            || self.at(SyntaxKind::UsesKeyword)
    }

    fn at_type_expression_boundary_kind(&self, kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::CommaToken
                | SyntaxKind::CloseParenToken
                | SyntaxKind::CloseBracketToken
                | SyntaxKind::EqualsToken
                | SyntaxKind::SemicolonToken
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::EndOfFileToken
        )
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
    fn parser_parses_path_self_prefix_tuple_slice_and_callable_type_expressions() {
        let cases = [
            ("Value", "Value", 0, 0),
            ("Self", "Self", 0, 0),
            ("&mut Value", "&mut Value", 1, 0),
            ("box Value", "box Value", 1, 0),
            ("(A, B,)", "(A, B,)", 2, 2),
            ("[Value]", "[Value]", 1, 0),
            ("func(value: Int) -> Bool", "func(value: Int) -> Bool", 0, 0),
        ];

        for (source_text, expected_text, nested_count, separator_count) in cases {
            let sources = source_store([source_text]);
            let snapshot = source(&sources, 0);

            let mut parser = Parser::new(snapshot);
            let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

            let expression = parser.parse_type_expression_until(&mut boundary);
            let diagnostics = parser.finish();

            assert_eq!(expression.full_text(), expected_text);
            assert_eq!(expression.type_expressions().count(), nested_count);
            assert_eq!(expression.separator_tokens().count(), separator_count);
            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }

    #[test]
    fn parser_skips_generic_arguments_inside_type_expressions_for_now() {
        let sources = source_store(["Vec<T>"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let expression = parser.parse_type_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let skipped_syntax = expression.skipped_syntax().collect::<Vec<_>>();

        let [generic_arguments] = skipped_syntax.as_slice() else {
            panic!("expected skipped generic arguments: {skipped_syntax:?}");
        };

        assert_eq!(expression.full_text(), "Vec<T>");
        assert_eq!(generic_arguments.full_text(), "<T>");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    #[test]
    fn parser_skips_callable_contract_clauses_inside_type_expressions_for_now() {
        let sources = source_store(["func() requires(valid)"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let expression = parser.parse_type_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let skipped_syntax = expression.skipped_syntax().collect::<Vec<_>>();

        let [contract_clause] = skipped_syntax.as_slice() else {
            panic!("expected skipped callable contract clause: {skipped_syntax:?}");
        };

        assert_eq!(expression.full_text(), "func() requires(valid)");
        assert_eq!(contract_clause.full_text(), "requires(valid)");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    #[test]
    fn parser_reports_missing_type_expression_inside_empty_brackets() {
        let sources = source_store(["[]"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let expression = parser.parse_type_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let inner_expressions = expression.type_expressions().collect::<Vec<_>>();

        let [inner] = inner_expressions.as_slice() else {
            panic!("expected missing inner type expression");
        };

        assert_eq!(expression.full_text(), "[]");
        assert!(inner.path().is_some());

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_recovers_unknown_type_expressions_without_looping() {
        let sources = source_store(["$;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_type_expression_until(&mut boundary);

        assert_eq!(parser.peek().kind(), SyntaxKind::SemicolonToken);

        let diagnostics = parser.finish();

        assert_eq!(expression.full_text(), "$");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }
}
