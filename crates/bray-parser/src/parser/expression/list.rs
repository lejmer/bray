use bray_syntax::{
    ArgumentListSyntax, ArgumentListSyntaxBuilder, ArgumentSyntax, ArrayExpressionSyntax,
    ArrayExpressionSyntaxBuilder, StructConstructionBodySyntax,
    StructConstructionBodySyntaxBuilder, StructFieldInitializerSyntax, SyntaxKind, SyntaxToken,
};

use crate::parser::separated::{
    SeparatedListSpec, SeparatedListSyntaxSink, separated_list_recovery_kinds,
};
use crate::parser::state::Parser;

use super::grammar::{
    ARGUMENT_LIST_TERMINATORS, EXPRESSION_START_KINDS, STRUCT_CONSTRUCTION_BODY_TERMINATORS,
    STRUCT_FIELD_INITIALIZER_START_KINDS, array_element_recovery_kinds,
};

impl Parser {
    pub(in crate::parser::expression) fn parse_array_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ArrayExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ArrayExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_open_bracket_token(self.expect(SyntaxKind::OpenBracketToken));

        if self.at(SyntaxKind::CloseBracketToken) {
            builder.push_expression(self.missing_expression());
        } else if self.at(SyntaxKind::EachKeyword) {
            builder
                .push_generator_iteration_expression(self.parse_generator_iteration_expression());

            self.recover_until_predicate(&mut builder, |parser| {
                parser.at(SyntaxKind::CloseBracketToken)
                    || parser.at(SyntaxKind::SemicolonToken)
                    || parser.at(SyntaxKind::EndOfFileToken)
                    || at_boundary(parser)
            });
        } else {
            self.parse_array_expression_items(&mut builder, at_boundary);
        }

        builder.push_close_bracket_token(self.expect(SyntaxKind::CloseBracketToken));

        builder.build()
    }

    fn parse_array_expression_items(
        &mut self,
        builder: &mut ArrayExpressionSyntaxBuilder,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        let mut at_element_boundary =
            |parser: &mut Parser| parser.at_array_expression_element_boundary(at_boundary);

        builder.push_expression(self.parse_expression_until(&mut at_element_boundary));

        if self.at(SyntaxKind::SemicolonToken) {
            builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

            let mut at_count_boundary = Parser::at_repeated_array_count_boundary;

            builder.push_expression(
                self.parse_non_assignment_expression_until(&mut at_count_boundary),
            );

            return;
        }

        self.parse_array_element_tail(builder, at_boundary);
    }

    fn parse_array_element_tail(
        &mut self,
        builder: &mut ArrayExpressionSyntaxBuilder,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        let recovery_kinds = array_element_recovery_kinds();

        while !self.at(SyntaxKind::CloseBracketToken)
            && !self.at(SyntaxKind::EndOfFileToken)
            && !at_boundary(self)
        {
            if self.at(SyntaxKind::CommaToken) || self.at_array_element_missing_separator() {
                builder.push_separator_token(self.expect(SyntaxKind::CommaToken));

                if self.at(SyntaxKind::CloseBracketToken) || self.at(SyntaxKind::EndOfFileToken) {
                    break;
                }

                let mut at_element_boundary =
                    |parser: &mut Parser| parser.at_array_expression_element_boundary(at_boundary);

                builder.push_expression(self.parse_expression_until(&mut at_element_boundary));
                continue;
            }

            if self.recover_until(builder, &recovery_kinds) {
                continue;
            }

            self.recover_current_token(builder);
        }
    }

    pub(in crate::parser::expression) fn parse_argument_list(&mut self) -> ArgumentListSyntax {
        let start = self.peek().full_range().start();

        let recovery_kinds = separated_list_recovery_kinds(
            &EXPRESSION_START_KINDS,
            SyntaxKind::CommaToken,
            &ARGUMENT_LIST_TERMINATORS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &ARGUMENT_LIST_TERMINATORS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = ArgumentListSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        self.parse_separated_list(&mut builder, spec, Parser::parse_argument);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_argument(&mut self) -> ArgumentSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ArgumentSyntax::builder(self.syntax_source(), start);

        if self.at_named_argument_start() {
            builder.push_identifier_token(self.parse_identifier());
            builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
        }

        let mut at_argument_boundary = Parser::at_argument_boundary;

        builder.push_expression(self.parse_expression_until(&mut at_argument_boundary));

        builder.build()
    }

    pub(in crate::parser::expression) fn parse_struct_construction_body(
        &mut self,
    ) -> StructConstructionBodySyntax {
        let start = self.peek().full_range().start();

        let recovery_kinds = separated_list_recovery_kinds(
            &STRUCT_FIELD_INITIALIZER_START_KINDS,
            SyntaxKind::CommaToken,
            &STRUCT_CONSTRUCTION_BODY_TERMINATORS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &STRUCT_CONSTRUCTION_BODY_TERMINATORS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = StructConstructionBodySyntax::builder(self.syntax_source(), start);

        builder.push_open_brace_token(self.expect(SyntaxKind::OpenBraceToken));
        self.parse_separated_list(&mut builder, spec, Parser::parse_struct_field_initializer);
        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));

        builder.build()
    }

    fn parse_struct_field_initializer(&mut self) -> StructFieldInitializerSyntax {
        let start = self.peek().full_range().start();
        let mut builder = StructFieldInitializerSyntax::builder(self.syntax_source(), start);

        builder.push_identifier_token(self.parse_identifier());
        builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));

        let mut at_initializer_boundary = Parser::at_struct_field_initializer_boundary;

        builder.push_expression(self.parse_expression_until(&mut at_initializer_boundary));

        builder.build()
    }
}

impl SeparatedListSyntaxSink<ArgumentSyntax> for ArgumentListSyntaxBuilder {
    fn push_item(&mut self, item: ArgumentSyntax) {
        ArgumentListSyntaxBuilder::push_argument(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        ArgumentListSyntaxBuilder::push_separator_token(self, separator);
    }
}

impl SeparatedListSyntaxSink<StructFieldInitializerSyntax> for StructConstructionBodySyntaxBuilder {
    fn push_item(&mut self, item: StructFieldInitializerSyntax) {
        StructConstructionBodySyntaxBuilder::push_field_initializer(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        StructConstructionBodySyntaxBuilder::push_separator_token(self, separator);
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::state::Parser;
    use crate::test_support::{diagnostic_kinds, source};

    #[test]
    fn parser_represents_missing_argument_separators_and_skipped_argument_tokens() {
        let sources = source_store(["call(first second, $, third);"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        let call = match expression.call_operations().next() {
            Some(call) => call,
            None => panic!("call operation should be present"),
        };

        let list = call.argument_list();
        let separators = list.separator_tokens().collect::<Vec<_>>();
        let skipped = list.skipped_syntax().collect::<Vec<_>>();

        assert_eq!(expression.full_text(), "call(first second, $, third)");
        assert!(separators.iter().any(|separator| separator.is_missing()));

        let [skipped_syntax] = skipped.as_slice() else {
            panic!("expected skipped argument syntax: {skipped:?}");
        };

        assert_eq!(skipped_syntax.full_text(), "$");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }
}
