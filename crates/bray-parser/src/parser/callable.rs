use bray_syntax::{
    CallableBodyBlockExpressionSyntax, CallableResultClauseSyntax, ParameterListSyntax,
    ParameterListSyntaxBuilder, ParameterModifiersSyntax, ParameterSyntax, ParameterSyntaxBuilder,
    SyntaxKind, SyntaxToken,
};

use super::separated::{SeparatedListSpec, SeparatedListSyntaxSink, separated_list_recovery_kinds};
use super::state::Parser;

const PARAMETER_START_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::PosKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::IdentifierToken,
];

const PARAMETER_LIST_TERMINATORS: [SyntaxKind; 9] = [
    SyntaxKind::CloseParenToken,
    SyntaxKind::ArrowToken,
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::UsesKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::EndOfFileToken,
];

const PARAMETER_TYPE_BOUNDARY_KINDS: [SyntaxKind; 12] = [
    SyntaxKind::EqualsToken,
    SyntaxKind::CommaToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::ArrowToken,
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::UsesKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const PARAMETER_DEFAULT_BOUNDARY_KINDS: [SyntaxKind; 11] = [
    SyntaxKind::CommaToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::ArrowToken,
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::UsesKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const CALLABLE_RESULT_TYPE_BOUNDARY_KINDS: [SyntaxKind; 8] = [
    SyntaxKind::RequiresKeyword,
    SyntaxKind::EnsuresKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::UsesKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_parameter_list(&mut self) -> ParameterListSyntax {
        let start = self.peek().full_range().start();

        let recovery_kinds = separated_list_recovery_kinds(
            &PARAMETER_START_KINDS,
            SyntaxKind::CommaToken,
            &PARAMETER_LIST_TERMINATORS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &PARAMETER_LIST_TERMINATORS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = ParameterListSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        self.parse_separated_list(&mut builder, spec, Parser::parse_parameter);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_parameter(&mut self) -> ParameterSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ParameterSyntax::builder(self.syntax_source(), start);

        builder.push_parameter_modifiers(self.parse_parameter_modifiers());
        builder.push_identifier_token(self.parse_identifier());
        builder.push_colon_token(self.expect(SyntaxKind::ColonToken));

        let mut at_type_boundary = Parser::at_parameter_type_boundary;

        builder.push_type_expression(self.parse_type_expression_until(&mut at_type_boundary));

        if self.at(SyntaxKind::EqualsToken) {
            builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
            // TODO(parser): Parse parameter default expressions once expression parsing is implemented.
            self.recover_parameter_default_expression(&mut builder);
        }

        builder.build()
    }

    fn parse_parameter_modifiers(&mut self) -> ParameterModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ParameterModifiersSyntax::builder(self.syntax_source(), start);

        while self.at(SyntaxKind::PosKeyword) || self.at(SyntaxKind::MutKeyword) {
            if self.at(SyntaxKind::PosKeyword) {
                builder.push_pos_token(self.expect(SyntaxKind::PosKeyword));
                continue;
            }

            builder.push_mut_token(self.expect(SyntaxKind::MutKeyword));
        }

        builder.build()
    }

    fn recover_parameter_default_expression(&mut self, builder: &mut ParameterSyntaxBuilder) {
        if self.at_parameter_default_boundary() {
            return;
        }

        self.recover_current_and_until_predicate(builder, Parser::at_parameter_default_boundary);
    }

    fn at_parameter_type_boundary(&mut self) -> bool {
        self.at_any(&PARAMETER_TYPE_BOUNDARY_KINDS) || self.at_probable_parameter_start()
    }

    fn at_parameter_default_boundary(&mut self) -> bool {
        self.at_any(&PARAMETER_DEFAULT_BOUNDARY_KINDS) || self.at_probable_parameter_start()
    }

    fn at_probable_parameter_start(&mut self) -> bool {
        let mut distance = 0;

        while matches!(
            self.lookahead(distance).kind(),
            SyntaxKind::PosKeyword | SyntaxKind::MutKeyword
        ) {
            distance += 1;
        }

        self.lookahead(distance).kind() == SyntaxKind::IdentifierToken
            && self.lookahead(distance + 1).kind() == SyntaxKind::ColonToken
    }

    pub(super) fn parse_callable_result_clause(&mut self) -> CallableResultClauseSyntax {
        self.parse_callable_result_clause_until(Parser::at_callable_result_type_boundary)
    }

    pub(super) fn parse_callable_result_clause_until(
        &mut self,
        mut at_result_type_boundary: impl FnMut(&mut Parser) -> bool,
    ) -> CallableResultClauseSyntax {
        let start = self.peek().full_range().start();
        let mut builder = CallableResultClauseSyntax::builder(self.syntax_source(), start);

        builder.push_arrow_token(self.expect(SyntaxKind::ArrowToken));

        builder
            .push_type_expression(self.parse_type_expression_until(&mut at_result_type_boundary));

        builder.build()
    }

    fn at_callable_result_type_boundary(&mut self) -> bool {
        self.at_any(&CALLABLE_RESULT_TYPE_BOUNDARY_KINDS)
    }

    pub(super) fn parse_callable_body_block_expression(
        &mut self,
    ) -> CallableBodyBlockExpressionSyntax {
        self.parse_callable_body_block_expression_until(|_| false)
    }

    pub(super) fn parse_callable_body_block_expression_until(
        &mut self,
        at_missing_body_boundary: impl FnMut(&mut Parser) -> bool,
    ) -> CallableBodyBlockExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = CallableBodyBlockExpressionSyntax::builder(self.syntax_source(), start);

        // TODO(parser): Parse block expressions once expression parsing is implemented.
        self.parse_skipped_braced_body_tokens(&mut builder, at_missing_body_boundary);

        builder.build()
    }
}

impl SeparatedListSyntaxSink<ParameterSyntax> for ParameterListSyntaxBuilder {
    fn push_item(&mut self, item: ParameterSyntax) {
        ParameterListSyntaxBuilder::push_parameter(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        ParameterListSyntaxBuilder::push_separator_token(self, separator);
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use super::super::state::Parser;
    use crate::test_support::{diagnostic_kinds, source};

    // TODO(parser): Update this when parameter default expressions are parsed.
    #[test]
    fn parser_parses_valid_parameter_lists() {
        let sources = source_store(["(pos value: Int = 1, mut tail: Bool,)"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_parameter_list();
        let diagnostics = parser.finish();
        let parameters = list.parameters().collect::<Vec<_>>();

        let [first, second] = parameters.as_slice() else {
            panic!("expected two parameters: {parameters:?}");
        };

        assert_eq!(list.full_text(), "(pos value: Int = 1, mut tail: Bool,)");
        assert_eq!(list.separator_tokens().count(), 2);

        assert_eq!(
            first
                .parameter_modifiers()
                .pos_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PosKeyword)
        );

        assert!(first.equals_token().is_some());
        assert_eq!(first.type_expression().full_text(), "Int ");

        assert_eq!(
            second
                .parameter_modifiers()
                .mut_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::MutKeyword)
        );

        assert_eq!(second.type_expression().full_text(), "Bool");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    #[test]
    fn parser_parameter_lists_represent_missing_separators() {
        let sources = source_store(["(first: Int mut second: Bool)"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_parameter_list();
        let diagnostics = parser.finish();
        let separators = list.separator_tokens().collect::<Vec<_>>();

        let [separator] = separators.as_slice() else {
            panic!("expected one separator token: {separators:?}");
        };

        assert_eq!(list.full_text(), "(first: Int mut second: Bool)");
        assert_eq!(list.parameters().count(), 2);
        assert!(separator.is_missing());
        assert_eq!(separator.kind(), SyntaxKind::CommaToken);

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_parameter_lists_recover_bad_tokens_without_losing_later_items() {
        let sources = source_store(["(first: Int, $, second: Bool)"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_parameter_list();
        let diagnostics = parser.finish();
        let parameters = list.parameters().collect::<Vec<_>>();
        let skipped_syntax = list.skipped_syntax().collect::<Vec<_>>();

        let [first, recovered, second] = parameters.as_slice() else {
            panic!("expected three parameter slots: {parameters:?}");
        };

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(list.full_text(), "(first: Int, $, second: Bool)");
        assert!(recovered.identifier_token().is_missing());
        assert_eq!(first.type_expression().full_text(), "Int");
        assert_eq!(skipped.full_text(), "$");
        assert_eq!(second.type_expression().full_text(), "Bool");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }
}
