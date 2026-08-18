use bray_syntax::{
    CallOperationSyntax, ConversionOperationSyntax, ElementIndexOperationSyntax, ExpressionSyntax,
    MemberAccessOperationSyntax, NullablePropagationOperationSyntax, PrimaryExpressionSyntax,
    SliceIndexOperationSyntax, SyntaxKind, TraitQualifiedMemberOperationSyntax,
};

use crate::parser::state::Parser;

#[derive(Clone, Copy)]
pub(super) enum PostfixOperationStart {
    MemberAccess,
    Index,
    Parenthesized,
    Generic,
    NullablePropagation,
    Conversion,
}

impl Parser {
    pub(in crate::parser::expression) fn parse_postfix_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let primary = self.parse_primary_expression_until(at_boundary);
        let mut expression = self.primary_to_expression(primary);

        loop {
            if at_boundary(self) {
                break;
            }

            let Some(operation) = postfix_operation_start(self.peek().kind()) else {
                break;
            };

            expression = match operation {
                PostfixOperationStart::MemberAccess => {
                    self.parse_member_access_postfix(expression)
                }
                PostfixOperationStart::Index if self.should_parse_slice_index_operation() => {
                    self.parse_slice_index_postfix(expression, at_boundary)
                }
                PostfixOperationStart::Index => {
                    self.parse_element_index_postfix(expression, at_boundary)
                }
                PostfixOperationStart::Parenthesized
                    if self.should_parse_trait_qualified_member_operation() =>
                {
                    self.parse_trait_qualified_member_postfix(expression)
                }
                PostfixOperationStart::Parenthesized => self.parse_call_postfix(expression),
                PostfixOperationStart::Generic if self.should_parse_explicit_generic_call() => {
                    self.parse_call_postfix(expression)
                }
                PostfixOperationStart::Generic
                    if self.should_parse_explicit_generic_application() =>
                {
                    self.parse_generic_application_postfix(expression)
                }
                PostfixOperationStart::Generic => break,
                PostfixOperationStart::NullablePropagation => {
                    self.parse_nullable_propagation_postfix(expression)
                }
                PostfixOperationStart::Conversion => {
                    self.parse_conversion_postfix(expression, at_boundary)
                }
            };
        }

        expression
    }

    pub(in crate::parser::expression) fn primary_to_expression(
        &mut self,
        primary: PrimaryExpressionSyntax,
    ) -> ExpressionSyntax {
        let start = primary.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_primary_expression(primary);

        builder.build()
    }

    fn parse_member_access_postfix(&mut self, expression: ExpressionSyntax) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_member_access_operation(self.parse_member_access_operation());

        builder.build()
    }

    fn parse_element_index_postfix(
        &mut self,
        expression: ExpressionSyntax,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_element_index_operation(self.parse_element_index_operation(at_boundary));

        builder.build()
    }

    fn parse_call_postfix(&mut self, expression: ExpressionSyntax) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_call_operation(self.parse_call_operation());

        builder.build()
    }

    fn parse_generic_application_postfix(
        &mut self,
        expression: ExpressionSyntax,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_generic_argument_list(self.parse_generic_argument_list());

        builder.build()
    }

    fn parse_slice_index_postfix(
        &mut self,
        expression: ExpressionSyntax,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_slice_index_operation(self.parse_slice_index_operation(at_boundary));

        builder.build()
    }

    fn parse_nullable_propagation_postfix(
        &mut self,
        expression: ExpressionSyntax,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_nullable_propagation_operation(self.parse_nullable_propagation_operation());

        builder.build()
    }

    fn parse_conversion_postfix(
        &mut self,
        expression: ExpressionSyntax,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_conversion_operation(self.parse_conversion_operation(at_boundary));

        builder.build()
    }

    fn parse_trait_qualified_member_postfix(
        &mut self,
        expression: ExpressionSyntax,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);

        builder
            .push_trait_qualified_member_operation(self.parse_trait_qualified_member_operation());

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_member_access_operation(
        &mut self,
    ) -> MemberAccessOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = MemberAccessOperationSyntax::builder(self.syntax_source(), start);

        builder.push_dot_token(self.expect(SyntaxKind::DotToken));

        match self.peek().kind() {
            SyntaxKind::DecimalIntegerLiteralToken => {
                builder
                    .push_tuple_element_index_token(self.consume_tuple_element_index_after_dot());
            }
            _ => builder.push_identifier_token(self.parse_identifier()),
        }

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_element_index_operation(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ElementIndexOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ElementIndexOperationSyntax::builder(self.syntax_source(), start);

        builder.push_open_bracket_token(self.expect(SyntaxKind::OpenBracketToken));

        builder.push_expression(
            self.parse_expression_until(&mut |parser| {
                parser.at_element_index_boundary(at_boundary)
            }),
        );

        builder.push_close_bracket_token(self.expect(SyntaxKind::CloseBracketToken));

        builder.build()
    }

    fn parse_call_operation(&mut self) -> CallOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = CallOperationSyntax::builder(self.syntax_source(), start);

        if self.at(SyntaxKind::LessToken) {
            builder.push_generic_argument_list(self.parse_generic_argument_list());
        }

        builder.push_argument_list(self.parse_argument_list());

        builder.build()
    }

    fn parse_slice_index_operation(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> SliceIndexOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SliceIndexOperationSyntax::builder(self.syntax_source(), start);

        builder.push_open_bracket_token(self.expect(SyntaxKind::OpenBracketToken));

        if !self.at(SyntaxKind::DotDotToken) {
            let mut at_start_boundary =
                |parser: &mut Parser| parser.at_slice_selector_start_boundary(at_boundary);

            builder.push_expression(self.parse_expression_until(&mut at_start_boundary));
        }

        builder.push_dot_dot_token(self.expect(SyntaxKind::DotDotToken));

        if !self.at(SyntaxKind::CloseBracketToken) && !at_boundary(self) {
            let mut at_end_boundary =
                |parser: &mut Parser| parser.at_slice_selector_end_boundary(at_boundary);

            builder.push_expression(self.parse_expression_until(&mut at_end_boundary));
        }

        builder.push_close_bracket_token(self.expect(SyntaxKind::CloseBracketToken));

        builder.build()
    }

    fn parse_nullable_propagation_operation(&mut self) -> NullablePropagationOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = NullablePropagationOperationSyntax::builder(self.syntax_source(), start);

        builder.push_question_token(self.expect(SyntaxKind::QuestionToken));

        builder.build()
    }

    fn parse_conversion_operation(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ConversionOperationSyntax {
        let start = self.peek().full_range().start();

        let mut builder = ConversionOperationSyntax::builder(self.syntax_source(), start);

        let mut at_type_boundary =
            |parser: &mut Parser| parser.at_conversion_type_boundary(at_boundary);

        builder.push_as_keyword(self.expect(SyntaxKind::AsKeyword));
        builder.push_type_expression(self.parse_type_expression_until(&mut at_type_boundary));

        builder.build()
    }

    fn parse_trait_qualified_member_operation(&mut self) -> TraitQualifiedMemberOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TraitQualifiedMemberOperationSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        builder.push_trait_application(self.parse_trait_application());
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));
        builder.push_member_access_operation(self.parse_member_access_operation());

        builder.build()
    }
}

pub(super) const fn postfix_operation_start(kind: SyntaxKind) -> Option<PostfixOperationStart> {
    match kind {
        SyntaxKind::DotToken => Some(PostfixOperationStart::MemberAccess),
        SyntaxKind::OpenBracketToken => Some(PostfixOperationStart::Index),
        SyntaxKind::OpenParenToken => Some(PostfixOperationStart::Parenthesized),
        SyntaxKind::LessToken => Some(PostfixOperationStart::Generic),
        SyntaxKind::QuestionToken => Some(PostfixOperationStart::NullablePropagation),
        SyntaxKind::AsKeyword => Some(PostfixOperationStart::Conversion),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{ExpressionSyntax, SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::state::Parser;
    use crate::test_support::{diagnostic_kinds, source};

    #[test]
    fn parser_parses_typed_access_call_index_slice_and_conversion_postfixes() {
        let sources = source_store(["target.call(1, named = value)[0][start..end]? as Result;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(
            expression.full_text(),
            "target.call(1, named = value)[0][start..end]? as Result"
        );

        assert_eq!(count_call_operations(&expression), 1);
        assert_eq!(count_element_index_operations(&expression), 1);
        assert_eq!(count_slice_index_operations(&expression), 1);
        assert_eq!(count_nullable_propagation_operations(&expression), 1);
        assert_eq!(count_conversion_operations(&expression), 1);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_trait_qualified_member_postfix() {
        let sources = source_store(["target(Display).format;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(expression.full_text(), "target(Display).format");
        assert_eq!(count_trait_qualified_member_operations(&expression), 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_contextually_scans_tuple_element_indices_after_dots() {
        let sources = source_store(["pair.0;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(expression.full_text(), "pair.0");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_explicit_generic_call_arguments() {
        let sources = source_store(["target.method<Result, 4>(value);"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        let Some(call) = expression.call_operations().next() else {
            panic!("generic application must produce a call operation");
        };

        let generic_lists = call.generic_argument_lists().collect::<Vec<_>>();

        let [generic_list] = generic_lists.as_slice() else {
            panic!("call must retain exactly one generic argument list: {generic_lists:?}");
        };

        let generic_arguments = generic_list.generic_arguments().collect::<Vec<_>>();

        let [type_argument, constant_argument] = generic_arguments.as_slice() else {
            panic!("call must retain both generic arguments: {generic_arguments:?}");
        };

        assert_eq!(expression.full_text(), "target.method<Result, 4>(value)");
        assert_eq!(type_argument.type_expressions().count(), 1);
        assert_eq!(constant_argument.expressions().count(), 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_direct_explicit_generic_call_arguments() {
        let sources = source_store(["target<Item, 4>(0);"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(expression.call_operations().count(), 1);
        assert!(expression.operator_token().is_none());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_bare_generic_access_path_arguments() {
        let sources = source_store(["Value<1>;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        let lists = expression.generic_argument_lists().collect::<Vec<_>>();

        let [arguments] = lists.as_slice() else {
            panic!("generic access path must retain one argument list: {lists:?}");
        };

        assert_eq!(expression.full_text(), "Value<1>");
        assert_eq!(arguments.generic_arguments().count(), 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_does_not_treat_comparison_as_a_generic_call() {
        let sources = source_store(["left < right;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(expression.full_text(), "left < right");
        assert_eq!(count_call_operations(&expression), 0);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_keeps_generic_applications_before_binary_operators() {
        let sources = source_store(["Value<1> == expected;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let operands = expression.expressions().collect::<Vec<_>>();

        let [left, _right] = operands.as_slice() else {
            panic!("expected equality expression operands: {operands:?}");
        };

        assert_eq!(
            expression.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::EqualsEqualsToken)
        );

        assert_eq!(left.generic_argument_lists().count(), 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_does_not_treat_logical_comparisons_as_a_generic_application() {
        let sources = source_store(["radix < 2 || radix > 36;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let operands = expression.expressions().collect::<Vec<_>>();

        let [left, right] = operands.as_slice() else {
            panic!("expected logical expression operands: {operands:?}");
        };

        assert_eq!(expression.full_text(), "radix < 2 || radix > 36");

        assert_eq!(
            expression.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::PipePipeToken)
        );

        assert_eq!(
            left.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::LessToken)
        );

        assert_eq!(
            right.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::GreaterToken)
        );

        assert!(expression.generic_argument_lists().next().is_none());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_recovers_missing_generic_call_argument_separators() {
        let sources = source_store(["target<Result 4>(value);"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        let Some(call) = expression.call_operations().next() else {
            panic!("recovered generic application must remain a call operation");
        };

        let Some(generic_list) = call.generic_argument_lists().next() else {
            panic!("recovered call must retain its generic argument list");
        };

        let separators = generic_list.separator_tokens().collect::<Vec<_>>();

        let [separator] = separators.as_slice() else {
            panic!("recovered list must retain one separator: {separators:?}");
        };

        assert_eq!(expression.full_text(), "target<Result 4>(value)");
        assert_eq!(generic_list.generic_arguments().count(), 2);
        assert!(separator.is_missing());

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    fn count_call_operations(expression: &ExpressionSyntax) -> usize {
        expression.call_operations().count()
            + expression
                .expressions()
                .map(|child| count_call_operations(&child))
                .sum::<usize>()
    }

    fn count_element_index_operations(expression: &ExpressionSyntax) -> usize {
        expression.element_index_operations().count()
            + expression
                .expressions()
                .map(|child| count_element_index_operations(&child))
                .sum::<usize>()
    }

    fn count_slice_index_operations(expression: &ExpressionSyntax) -> usize {
        expression.slice_index_operations().count()
            + expression
                .expressions()
                .map(|child| count_slice_index_operations(&child))
                .sum::<usize>()
    }

    fn count_nullable_propagation_operations(expression: &ExpressionSyntax) -> usize {
        expression.nullable_propagation_operations().count()
            + expression
                .expressions()
                .map(|child| count_nullable_propagation_operations(&child))
                .sum::<usize>()
    }

    fn count_conversion_operations(expression: &ExpressionSyntax) -> usize {
        expression.conversion_operations().count()
            + expression
                .expressions()
                .map(|child| count_conversion_operations(&child))
                .sum::<usize>()
    }

    fn count_trait_qualified_member_operations(expression: &ExpressionSyntax) -> usize {
        expression.trait_qualified_member_operations().count()
            + expression
                .expressions()
                .map(|child| count_trait_qualified_member_operations(&child))
                .sum::<usize>()
    }
}
