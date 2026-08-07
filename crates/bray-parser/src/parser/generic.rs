use bray_syntax::{
    GenericArgumentListSyntax, GenericArgumentListSyntaxBuilder, GenericArgumentSyntax,
    GenericConstParameterSyntax, GenericParameterListSyntax, GenericParameterListSyntaxBuilder,
    GenericTypeParameterSyntax, SyntaxKind, SyntaxToken, TypeFormArgumentListSyntax,
    TypeFormArgumentListSyntaxBuilder, TypeFormArgumentSyntax,
};

use super::delimiter::DelimiterDepth;
use super::separated::{SeparatedListSpec, SeparatedListSyntaxSink, separated_list_recovery_kinds};
use super::state::Parser;

const GENERIC_PARAMETER_START_KINDS: [SyntaxKind; 2] =
    [SyntaxKind::ConstKeyword, SyntaxKind::IdentifierToken];

const GENERIC_PARAMETER_LIST_TERMINATORS: [SyntaxKind; 10] = [
    SyntaxKind::GreaterToken,
    SyntaxKind::GreaterGreaterToken,
    SyntaxKind::OpenParenToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::WithKeyword,
    SyntaxKind::EqualsToken,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const GENERIC_CONST_PARAMETER_TYPE_BOUNDARY_KINDS: [SyntaxKind; 11] = [
    SyntaxKind::CommaToken,
    SyntaxKind::GreaterToken,
    SyntaxKind::GreaterGreaterToken,
    SyntaxKind::OpenParenToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::WithKeyword,
    SyntaxKind::EqualsToken,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const GENERIC_ARGUMENT_START_KINDS: [SyntaxKind; 24] = [
    SyntaxKind::AmpersandToken,
    SyntaxKind::BangToken,
    SyntaxKind::BinaryIntegerLiteralToken,
    SyntaxKind::BoxKeyword,
    SyntaxKind::CharacterLiteralToken,
    SyntaxKind::DecimalIntegerLiteralToken,
    SyntaxKind::FalseKeyword,
    SyntaxKind::FuncKeyword,
    SyntaxKind::HexadecimalIntegerLiteralToken,
    SyntaxKind::IdentifierToken,
    SyntaxKind::ImaginaryLiteralToken,
    SyntaxKind::MinusToken,
    SyntaxKind::NoneKeyword,
    SyntaxKind::OpenBracketToken,
    SyntaxKind::OpenParenToken,
    SyntaxKind::RealLiteralToken,
    SyntaxKind::SelfTypeKeyword,
    SyntaxKind::SelfValueKeyword,
    SyntaxKind::StringLiteralToken,
    SyntaxKind::TildeToken,
    SyntaxKind::TrueKeyword,
    SyntaxKind::TupleElementIndexToken,
    SyntaxKind::UnitKeyword,
    SyntaxKind::ViewKeyword,
];

const GENERIC_ARGUMENT_LIST_TERMINATORS: [SyntaxKind; 9] = [
    SyntaxKind::GreaterToken,
    SyntaxKind::GreaterGreaterToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::CloseBracketToken,
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const TYPE_FORM_ARGUMENT_LIST_TERMINATORS: [SyntaxKind; 9] = [
    SyntaxKind::CloseBracketToken,
    SyntaxKind::GreaterToken,
    SyntaxKind::GreaterGreaterToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

enum GenericParameterSyntaxItem {
    Type(GenericTypeParameterSyntax),
    Const(GenericConstParameterSyntax),
}

impl Parser {
    pub(super) fn parse_generic_parameter_list(&mut self) -> GenericParameterListSyntax {
        let start = self.peek().full_range().start();

        let recovery_kinds = separated_list_recovery_kinds(
            &GENERIC_PARAMETER_START_KINDS,
            SyntaxKind::CommaToken,
            &GENERIC_PARAMETER_LIST_TERMINATORS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &GENERIC_PARAMETER_LIST_TERMINATORS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = GenericParameterListSyntax::builder(self.syntax_source(), start);

        builder.push_less_token(self.expect(SyntaxKind::LessToken));
        self.parse_separated_list(&mut builder, spec, Parser::parse_generic_parameter);
        builder.push_greater_token(self.expect_generic_close());

        builder.build()
    }

    fn parse_generic_parameter(&mut self) -> GenericParameterSyntaxItem {
        if self.at(SyntaxKind::ConstKeyword) {
            return GenericParameterSyntaxItem::Const(self.parse_generic_const_parameter());
        }

        GenericParameterSyntaxItem::Type(self.parse_generic_type_parameter())
    }

    fn parse_generic_type_parameter(&mut self) -> GenericTypeParameterSyntax {
        let start = self.peek().full_range().start();
        let mut builder = GenericTypeParameterSyntax::builder(self.syntax_source(), start);

        builder.push_identifier_token(self.parse_identifier());

        builder.build()
    }

    fn parse_generic_const_parameter(&mut self) -> GenericConstParameterSyntax {
        let start = self.peek().full_range().start();
        let mut builder = GenericConstParameterSyntax::builder(self.syntax_source(), start);

        builder.push_const_keyword(self.expect(SyntaxKind::ConstKeyword));
        let mut at_type_boundary = Parser::at_generic_const_parameter_type_boundary;

        builder.push_typed_identifier(self.parse_typed_identifier_until(&mut at_type_boundary));

        builder.build()
    }

    fn at_generic_const_parameter_type_boundary(&mut self) -> bool {
        self.at_any(&GENERIC_CONST_PARAMETER_TYPE_BOUNDARY_KINDS)
    }

    pub(super) fn parse_generic_argument_list(&mut self) -> GenericArgumentListSyntax {
        let start = self.peek().full_range().start();

        let recovery_kinds = separated_list_recovery_kinds(
            &GENERIC_ARGUMENT_START_KINDS,
            SyntaxKind::CommaToken,
            &GENERIC_ARGUMENT_LIST_TERMINATORS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &GENERIC_ARGUMENT_LIST_TERMINATORS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = GenericArgumentListSyntax::builder(self.syntax_source(), start);

        builder.push_less_token(self.expect(SyntaxKind::LessToken));
        self.parse_separated_list(&mut builder, spec, Parser::parse_generic_argument);
        builder.push_greater_token(self.expect_generic_close());

        builder.build()
    }

    fn parse_generic_argument(&mut self) -> GenericArgumentSyntax {
        let start = self.peek().full_range().start();
        let mut builder = GenericArgumentSyntax::builder(self.syntax_source(), start);

        if self.should_parse_generic_argument_as_expression(&GENERIC_ARGUMENT_LIST_TERMINATORS) {
            let mut at_boundary = Parser::at_generic_argument_boundary;

            builder.push_expression(self.parse_non_assignment_expression_until(&mut at_boundary));
        } else {
            let mut at_boundary = Parser::at_generic_argument_boundary;

            builder.push_type_expression(self.parse_type_expression_until(&mut at_boundary));
        }

        builder.build()
    }

    pub(super) fn parse_type_form_argument_list(&mut self) -> TypeFormArgumentListSyntax {
        let start = self.peek().full_range().start();

        let recovery_kinds = separated_list_recovery_kinds(
            &GENERIC_ARGUMENT_START_KINDS,
            SyntaxKind::CommaToken,
            &TYPE_FORM_ARGUMENT_LIST_TERMINATORS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &TYPE_FORM_ARGUMENT_LIST_TERMINATORS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = TypeFormArgumentListSyntax::builder(self.syntax_source(), start);

        builder.push_open_bracket_token(self.expect(SyntaxKind::OpenBracketToken));
        self.parse_separated_list(&mut builder, spec, Parser::parse_type_form_argument);
        builder.push_close_bracket_token(self.expect(SyntaxKind::CloseBracketToken));

        builder.build()
    }

    fn parse_type_form_argument(&mut self) -> TypeFormArgumentSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeFormArgumentSyntax::builder(self.syntax_source(), start);

        if self.should_parse_generic_argument_as_expression(&TYPE_FORM_ARGUMENT_LIST_TERMINATORS) {
            let mut at_boundary = Parser::at_type_form_argument_boundary;

            builder.push_expression(self.parse_non_assignment_expression_until(&mut at_boundary));
        } else {
            let mut at_boundary = Parser::at_type_form_argument_boundary;

            builder.push_type_expression(self.parse_type_expression_until(&mut at_boundary));
        }

        builder.build()
    }

    fn at_generic_argument_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CommaToken) || self.at_any(&GENERIC_ARGUMENT_LIST_TERMINATORS)
    }

    fn at_type_form_argument_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CommaToken) || self.at_any(&TYPE_FORM_ARGUMENT_LIST_TERMINATORS)
    }

    fn should_parse_generic_argument_as_expression(&mut self, terminators: &[SyntaxKind]) -> bool {
        let kind = self.peek().kind();

        if at_generic_argument_expression_only_start(kind) {
            return true;
        }

        self.scan_ahead(|scan| scan.scan_generic_argument_has_expression_operator(terminators))
    }

    fn scan_generic_argument_has_expression_operator(
        &mut self,
        terminators: &[SyntaxKind],
    ) -> bool {
        let mut depth = DelimiterDepth::default();

        while !self.at(SyntaxKind::EndOfFileToken) {
            let kind = self.peek().kind();

            let at_outer_boundary = depth.is_at_root()
                && (kind == SyntaxKind::CommaToken || terminators.contains(&kind));

            if at_outer_boundary {
                return false;
            }

            if depth.is_at_root() && at_generic_argument_expression_operator(kind) {
                return true;
            }

            depth.observe_grouping_or_angle(kind);
            self.consume();
        }

        false
    }
}

impl SeparatedListSyntaxSink<GenericParameterSyntaxItem> for GenericParameterListSyntaxBuilder {
    fn push_item(&mut self, item: GenericParameterSyntaxItem) {
        match item {
            GenericParameterSyntaxItem::Type(parameter) => {
                GenericParameterListSyntaxBuilder::push_generic_type_parameter(self, parameter);
            }
            GenericParameterSyntaxItem::Const(parameter) => {
                GenericParameterListSyntaxBuilder::push_generic_const_parameter(self, parameter);
            }
        }
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        GenericParameterListSyntaxBuilder::push_separator_token(self, separator);
    }
}

impl SeparatedListSyntaxSink<GenericArgumentSyntax> for GenericArgumentListSyntaxBuilder {
    fn push_item(&mut self, item: GenericArgumentSyntax) {
        GenericArgumentListSyntaxBuilder::push_generic_argument(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        GenericArgumentListSyntaxBuilder::push_separator_token(self, separator);
    }
}

impl SeparatedListSyntaxSink<TypeFormArgumentSyntax> for TypeFormArgumentListSyntaxBuilder {
    fn push_item(&mut self, item: TypeFormArgumentSyntax) {
        TypeFormArgumentListSyntaxBuilder::push_type_form_argument(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        TypeFormArgumentListSyntaxBuilder::push_separator_token(self, separator);
    }
}

fn at_generic_argument_expression_only_start(kind: SyntaxKind) -> bool {
    kind.is_literal()
        || matches!(
            kind,
            SyntaxKind::BangToken
                | SyntaxKind::FalseKeyword
                | SyntaxKind::MinusToken
                | SyntaxKind::NoneKeyword
                | SyntaxKind::SelfValueKeyword
                | SyntaxKind::TildeToken
                | SyntaxKind::TrueKeyword
                | SyntaxKind::TupleElementIndexToken
        )
}

fn at_generic_argument_expression_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::AmpersandAmpersandToken
            | SyntaxKind::BangEqualsToken
            | SyntaxKind::CaretToken
            | SyntaxKind::EqualsEqualsToken
            | SyntaxKind::GreaterEqualsToken
            | SyntaxKind::LessEqualsToken
            | SyntaxKind::MinusToken
            | SyntaxKind::PercentToken
            | SyntaxKind::PipePipeToken
            | SyntaxKind::PipeToken
            | SyntaxKind::PlusToken
            | SyntaxKind::SlashToken
            | SyntaxKind::StarStarToken
            | SyntaxKind::StarToken
    )
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::SyntaxText;
    use bray_testing::test_source_store as source_store;

    use super::super::state::Parser;
    use crate::test_support::{diagnostic_kinds, source};

    #[test]
    fn parser_parses_generic_parameter_lists() {
        let sources = source_store(["<T, const N: Int,>"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_generic_parameter_list();
        let diagnostics = parser.finish();

        assert_eq!(list.full_text(), "<T, const N: Int,>");
        assert_eq!(list.generic_type_parameters().count(), 1);
        assert_eq!(list.generic_const_parameters().count(), 1);
        assert_eq!(list.separator_tokens().count(), 2);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_represents_missing_generic_parameter_separators() {
        let sources = source_store(["<T const N: Int>"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_generic_parameter_list();
        let diagnostics = parser.finish();
        let separators = list.separator_tokens().collect::<Vec<_>>();

        let [separator] = separators.as_slice() else {
            panic!("expected one separator token: {separators:?}");
        };

        assert_eq!(list.full_text(), "<T const N: Int>");
        assert!(separator.is_missing());

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_recovers_bad_tokens_inside_generic_parameter_lists() {
        let sources = source_store(["<T, $, U>"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_generic_parameter_list();
        let diagnostics = parser.finish();
        let skipped_syntax = list.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(list.full_text(), "<T, $, U>");
        assert_eq!(list.generic_type_parameters().count(), 3);
        assert_eq!(skipped.full_text(), "$");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedToken,
            ]
        );
    }

    #[test]
    fn parser_parses_generic_argument_lists() {
        let sources = source_store(["<T, 1 + 2,>"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_generic_argument_list();
        let diagnostics = parser.finish();
        let arguments = list.generic_arguments().collect::<Vec<_>>();

        let [type_argument, constant_argument] = arguments.as_slice() else {
            panic!("expected two generic arguments: {arguments:?}");
        };

        assert_eq!(list.full_text(), "<T, 1 + 2,>");
        assert_eq!(list.separator_tokens().count(), 2);
        assert_eq!(type_argument.type_expressions().count(), 1);
        assert_eq!(constant_argument.expressions().count(), 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_splits_adjacent_nested_generic_closes() {
        let sources = source_store(["<MoveCursor<i32>>"]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);

        let list = parser.parse_generic_argument_list();
        let diagnostics = parser.finish();

        assert_eq!(list.full_text(), "<MoveCursor<i32>>");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_treats_unit_as_a_type_generic_argument() {
        let sources = source_store(["<unit>"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_generic_argument_list();
        let diagnostics = parser.finish();
        let arguments = list.generic_arguments().collect::<Vec<_>>();

        let [argument] = arguments.as_slice() else {
            panic!("expected one generic argument");
        };

        assert_eq!(argument.type_expressions().count(), 1);
        assert_eq!(argument.expressions().count(), 0);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_type_form_argument_lists() {
        let sources = source_store(["[Heap, 1]"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let list = parser.parse_type_form_argument_list();
        let diagnostics = parser.finish();
        let arguments = list.type_form_arguments().collect::<Vec<_>>();

        let [type_argument, constant_argument] = arguments.as_slice() else {
            panic!("expected two type-form arguments: {arguments:?}");
        };

        assert_eq!(list.full_text(), "[Heap, 1]");
        assert_eq!(list.separator_tokens().count(), 1);
        assert_eq!(type_argument.type_expressions().count(), 1);
        assert_eq!(constant_argument.expressions().count(), 1);
        assert!(diagnostics.is_empty());
    }
}
