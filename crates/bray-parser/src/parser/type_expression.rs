use bray_syntax::{PathSyntax, SyntaxKind, SyntaxToken, TypeExpressionSyntax};

use super::callable::CALLABLE_FORM_START_KINDS;
use super::state::Parser;

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
            SyntaxKind::ViewKeyword => self.parse_view_type_expression(),
            kind if self.is_callable_type_expression_start_kind(kind)
                && self.should_parse_callable_type_expression() =>
            {
                self.parse_callable_type_expression(at_boundary)
            }
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

        if self.at(SyntaxKind::OpenBracketToken) {
            builder.push_type_form_argument_list(self.parse_type_form_argument_list());
        }

        builder.push_type_expression(self.parse_type_expression_until(at_boundary));

        builder.build()
    }

    fn parse_view_type_expression(&mut self) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_view_keyword(self.expect(SyntaxKind::ViewKeyword));
        builder.push_trait_application(self.parse_trait_application());

        builder.build()
    }

    fn parse_callable_type_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_callable_directives(self.parse_callable_directives());
        builder.push_callable_modifiers(self.parse_callable_modifiers());
        builder.push_func_keyword(self.expect(SyntaxKind::FuncKeyword));
        builder.push_parameter_list(self.parse_parameter_list());

        if self.at(SyntaxKind::ArrowToken) {
            builder.push_callable_result_clause(self.parse_callable_result_clause_until(
                |parser| parser.at_callable_type_tail_boundary(at_boundary),
            ));
        }

        if self.at_callable_contract_clause_start() {
            self.parse_callable_contract_clauses(
                &mut builder,
                Parser::at_callable_type_contract_boundary,
            );
        }

        builder.build()
    }

    fn parse_postfix_type_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeExpressionSyntax {
        let mut expression = self.parse_type_primary_expression(at_boundary);

        loop {
            if self.at(SyntaxKind::LessToken) {
                expression = self.parse_generic_type_operation(expression);
                continue;
            }

            if self.at(SyntaxKind::QuestionToken) {
                expression = self.parse_nullable_type_operation(expression);
                continue;
            }

            if self.at(SyntaxKind::OpenParenToken) {
                expression = self.parse_qualified_type_member_operation(expression);
                continue;
            }

            break;
        }

        expression
    }

    fn parse_generic_type_operation(
        &mut self,
        expression: TypeExpressionSyntax,
    ) -> TypeExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_type_expression(expression);
        builder.push_generic_argument_list(self.parse_generic_argument_list());

        builder.build()
    }

    fn parse_nullable_type_operation(
        &mut self,
        expression: TypeExpressionSyntax,
    ) -> TypeExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_type_expression(expression);
        builder.push_question_token(self.expect(SyntaxKind::QuestionToken));

        builder.build()
    }

    fn parse_qualified_type_member_operation(
        &mut self,
        expression: TypeExpressionSyntax,
    ) -> TypeExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = TypeExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_type_expression(expression);
        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        builder.push_trait_application(self.parse_trait_application());
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));
        builder.push_dot_token(self.expect(SyntaxKind::DotToken));
        builder.push_identifier_token(self.parse_identifier());

        builder.build()
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

            let mut at_size_boundary =
                |parser: &mut Parser| parser.at_any(&ARRAY_SIZE_BOUNDARY_KINDS);

            builder
                .push_expression(self.parse_non_assignment_expression_until(&mut at_size_boundary));
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

    fn at_callable_type_contract_boundary(&mut self) -> bool {
        matches!(
            self.peek().kind(),
            SyntaxKind::CloseBracketToken
                | SyntaxKind::EqualsToken
                | SyntaxKind::SemicolonToken
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::EndOfFileToken
        )
    }

    fn is_callable_type_expression_start_kind(&self, kind: SyntaxKind) -> bool {
        CALLABLE_FORM_START_KINDS.contains(&kind)
    }

    fn should_parse_callable_type_expression(&mut self) -> bool {
        self.scan_ahead(|scan| {
            scan.consume_callable_directives_for_scan();
            scan.consume_callable_modifiers_for_scan();

            scan.at(SyntaxKind::FuncKeyword)
        })
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
    fn parser_parses_generic_arguments_inside_type_expressions() {
        let sources = source_store(["Vec<Map<T>, 1 + 2>"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let expression = parser.parse_type_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let generic_argument_lists = expression.generic_argument_lists().collect::<Vec<_>>();

        let [generic_arguments] = generic_argument_lists.as_slice() else {
            panic!("expected generic argument list: {generic_argument_lists:?}");
        };

        let arguments = generic_arguments.generic_arguments().collect::<Vec<_>>();

        let [type_argument, constant_argument] = arguments.as_slice() else {
            panic!("expected two generic arguments: {arguments:?}");
        };

        assert_eq!(expression.full_text(), "Vec<Map<T>, 1 + 2>");
        assert_eq!(generic_arguments.full_text(), "<Map<T>, 1 + 2>");
        assert_eq!(generic_arguments.separator_tokens().count(), 1);
        assert_eq!(type_argument.type_expressions().count(), 1);
        assert_eq!(constant_argument.expressions().count(), 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_view_nullable_and_qualified_type_expressions() {
        let cases = [
            ("view core.Display<T>", 0, 1, 0),
            ("Value?(Display).Output", 1, 1, 1),
        ];

        for (source_text, nested_count, trait_application_count, question_count) in cases {
            let sources = source_store([source_text]);
            let snapshot = source(&sources, 0);

            let mut parser = Parser::new(snapshot);
            let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

            let expression = parser.parse_type_expression_until(&mut boundary);
            let diagnostics = parser.finish();

            assert_eq!(expression.full_text(), source_text);
            assert_eq!(expression.type_expressions().count(), nested_count);

            assert_eq!(
                expression.trait_applications().count(),
                trait_application_count
            );

            assert_eq!(
                expression
                    .tokens()
                    .filter(|token| token.kind() == SyntaxKind::QuestionToken)
                    .count(),
                question_count
            );

            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }

    #[test]
    fn parser_parses_callable_type_directives_and_modifiers() {
        let sources = source_store(["@abi(\"C\") async trusted const func(value: Int) -> Bool"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let expression = parser.parse_type_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let callable_directives = expression.callable_directives().collect::<Vec<_>>();
        let callable_modifiers = expression.callable_modifiers().collect::<Vec<_>>();

        let [directives] = callable_directives.as_slice() else {
            panic!("expected callable directives: {callable_directives:?}");
        };

        let [modifiers] = callable_modifiers.as_slice() else {
            panic!("expected callable modifiers: {callable_modifiers:?}");
        };

        assert_eq!(
            expression.full_text(),
            "@abi(\"C\") async trusted const func(value: Int) -> Bool"
        );

        assert_eq!(directives.abi_directives().count(), 1);

        assert!(modifiers.async_token().is_some());
        assert!(modifiers.trusted_token().is_some());
        assert!(modifiers.const_token().is_some());

        assert_eq!(expression.parameter_lists().count(), 1);
        assert_eq!(expression.callable_result_clauses().count(), 1);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_type_form_arguments_inside_type_expressions() {
        let sources = source_store(["box[Heap, 1] Point"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let expression = parser.parse_type_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let type_form_argument_lists = expression.type_form_argument_lists().collect::<Vec<_>>();

        let [type_form_arguments] = type_form_argument_lists.as_slice() else {
            panic!("expected type-form argument list: {type_form_argument_lists:?}");
        };

        let arguments = type_form_arguments
            .type_form_arguments()
            .collect::<Vec<_>>();

        let [type_argument, constant_argument] = arguments.as_slice() else {
            panic!("expected two type-form arguments: {arguments:?}");
        };

        assert_eq!(expression.full_text(), "box[Heap, 1] Point");
        assert_eq!(type_form_arguments.full_text(), "[Heap, 1] ");
        assert_eq!(type_form_arguments.separator_tokens().count(), 1);
        assert_eq!(type_argument.type_expressions().count(), 1);
        assert_eq!(constant_argument.expressions().count(), 1);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_callable_contract_clauses_inside_type_expressions() {
        let sources = source_store(["func() requires(valid) uses(core.io)"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let expression = parser.parse_type_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(
            expression.full_text(),
            "func() requires(valid) uses(core.io)"
        );
        assert_eq!(expression.requires_clauses().count(), 1);
        assert_eq!(expression.uses_clauses().count(), 1);
        assert_eq!(expression.skipped_syntax().count(), 0);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_callable_type_contract_recovery_stops_before_following_clause() {
        let sources = source_store(["func() requires(valid ensures(done)"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);

        let expression = parser.parse_type_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(
            expression.full_text(),
            "func() requires(valid ensures(done)"
        );
        assert_eq!(expression.requires_clauses().count(), 1);
        assert_eq!(expression.ensures_clauses().count(), 1);
        assert_eq!(expression.skipped_syntax().count(), 0);

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
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
