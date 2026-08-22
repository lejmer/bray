use bray_syntax::{
    ExpressionSyntax, PredicateDeclarationSyntax, PredicateDeclarationSyntaxBuilder,
    PredicateModifiersSyntax, PredicateParameterListSyntax, PredicateParameterListSyntaxBuilder,
    PredicateParameterSyntax, SyntaxKind, SyntaxToken, TraitPredicateMemberDeclarationSyntax,
    TraitPredicateMemberDeclarationSyntaxBuilder, TraitPredicateMemberModifiersSyntax,
};

use super::member::MEMBER_KEYWORD_RECOVERY_KINDS;
use super::module::MODULE_ITEM_START_KINDS;
use super::separated::{SeparatedListSpec, SeparatedListSyntaxSink, separated_list_recovery_kinds};
use super::state::Parser;

const PREDICATE_DECLARATION_START_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PredicateKeyword,
];

const PREDICATE_PARAMETER_LIST_TERMINATORS: [SyntaxKind; 5] = [
    SyntaxKind::CloseParenToken,
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const PREDICATE_PARAMETER_START_KINDS: [SyntaxKind; 1] = [SyntaxKind::IdentifierToken];

const PREDICATE_PARAMETER_TYPE_BOUNDARY_KINDS: [SyntaxKind; 6] = [
    SyntaxKind::CommaToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const PREDICATE_BODY_BOUNDARY_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_predicate_declaration(&mut self) -> PredicateDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PredicateDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_predicate_modifiers(self.parse_predicate_modifiers());
        builder.push_predicate_keyword(self.expect(SyntaxKind::PredicateKeyword));
        builder.push_identifier_token(self.parse_identifier());

        if self.at(SyntaxKind::LessToken) {
            builder.push_generic_parameter_list(self.parse_generic_parameter_list());
        }

        builder.push_predicate_parameter_list(self.parse_predicate_parameter_list());
        self.parse_predicate_declaration_tail(&mut builder);

        builder.build()
    }

    pub(super) fn parse_trait_predicate_member_declaration(
        &mut self,
    ) -> TraitPredicateMemberDeclarationSyntax {
        let start = self.peek().full_range().start();

        let mut builder =
            TraitPredicateMemberDeclarationSyntax::builder(self.syntax_source(), start);

        builder
            .push_trait_predicate_member_modifiers(self.parse_trait_predicate_member_modifiers());

        builder.push_predicate_keyword(self.expect(SyntaxKind::PredicateKeyword));
        builder.push_identifier_token(self.parse_identifier());
        builder.push_predicate_parameter_list(self.parse_predicate_parameter_list());

        self.parse_predicate_declaration_tail(&mut builder);

        builder.build()
    }

    fn parse_predicate_modifiers(&mut self) -> PredicateModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PredicateModifiersSyntax::builder(self.syntax_source(), start);

        while self.at_visibility_modifier() || self.at(SyntaxKind::TrustedKeyword) {
            if self.at_visibility_modifier() {
                builder.push_visibility_token(self.parse_visibility_modifier());
                continue;
            }

            builder.push_trusted_token(self.expect(SyntaxKind::TrustedKeyword));
        }

        builder.build()
    }

    fn parse_trait_predicate_member_modifiers(&mut self) -> TraitPredicateMemberModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TraitPredicateMemberModifiersSyntax::builder(self.syntax_source(), start);

        while self.at(SyntaxKind::TrustedKeyword) {
            builder.push_trusted_token(self.expect(SyntaxKind::TrustedKeyword));
        }

        builder.build()
    }

    fn parse_predicate_parameter_list(&mut self) -> PredicateParameterListSyntax {
        let start = self.peek().full_range().start();

        let recovery_kinds = separated_list_recovery_kinds(
            &PREDICATE_PARAMETER_START_KINDS,
            SyntaxKind::CommaToken,
            &PREDICATE_PARAMETER_LIST_TERMINATORS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &PREDICATE_PARAMETER_LIST_TERMINATORS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = PredicateParameterListSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        self.parse_separated_list(&mut builder, spec, Parser::parse_predicate_parameter);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_predicate_parameter(&mut self) -> PredicateParameterSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PredicateParameterSyntax::builder(self.syntax_source(), start);

        let mut at_type_boundary = Parser::at_predicate_parameter_type_boundary;

        builder.push_typed_identifier(self.parse_typed_identifier_until(&mut at_type_boundary));

        builder.build()
    }

    fn parse_predicate_declaration_tail(&mut self, builder: &mut impl PredicateSyntaxSink) {
        if self.at(SyntaxKind::EqualsToken) {
            builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));

            let mut at_body_boundary = Parser::at_predicate_body_boundary;

            builder
                .push_expression(self.parse_non_assignment_expression_until(&mut at_body_boundary));
        }

        self.recover_until_predicate_declaration_end(builder);
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));
    }

    fn at_predicate_parameter_type_boundary(&mut self) -> bool {
        self.at_any(&PREDICATE_PARAMETER_TYPE_BOUNDARY_KINDS)
            || self.at_probable_predicate_parameter_start()
    }

    fn at_probable_predicate_parameter_start(&mut self) -> bool {
        self.at(SyntaxKind::IdentifierToken) && self.lookahead(1).kind() == SyntaxKind::ColonToken
    }

    fn at_predicate_body_boundary(&mut self) -> bool {
        self.at_any(&PREDICATE_BODY_BOUNDARY_KINDS)
            || self.at_any(&MODULE_ITEM_START_KINDS)
            || self.at_any(&MEMBER_KEYWORD_RECOVERY_KINDS)
    }

    fn recover_until_predicate_declaration_end(&mut self, builder: &mut impl PredicateSyntaxSink) {
        self.recover_until_predicate(builder, Parser::at_predicate_body_boundary);
    }

    pub(super) fn should_parse_predicate_declaration(&mut self) -> bool {
        if !self.at_any(&PREDICATE_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_predicate_modifiers_for_scan();

            scan.at(SyntaxKind::PredicateKeyword)
        })
    }

    pub(super) fn should_parse_trait_predicate_member_declaration(&mut self) -> bool {
        if !self.at(SyntaxKind::TrustedKeyword) && !self.at(SyntaxKind::PredicateKeyword) {
            return false;
        }

        self.scan_ahead(|scan| {
            while scan.at(SyntaxKind::TrustedKeyword) {
                scan.consume();
            }

            scan.at(SyntaxKind::PredicateKeyword)
        })
    }

    fn consume_predicate_modifiers_for_scan(&mut self) {
        while self.at_visibility_modifier() || self.at(SyntaxKind::TrustedKeyword) {
            self.consume();
        }
    }
}

trait PredicateSyntaxSink: crate::parser::recovery::RecoverySyntaxSink {
    fn push_equals_token(&mut self, token: SyntaxToken);

    fn push_expression(&mut self, expression: ExpressionSyntax);

    fn push_semicolon_token(&mut self, token: SyntaxToken);
}

impl PredicateSyntaxSink for PredicateDeclarationSyntaxBuilder {
    fn push_equals_token(&mut self, token: SyntaxToken) {
        PredicateDeclarationSyntaxBuilder::push_equals_token(self, token);
    }

    fn push_expression(&mut self, expression: ExpressionSyntax) {
        PredicateDeclarationSyntaxBuilder::push_expression(self, expression);
    }

    fn push_semicolon_token(&mut self, token: SyntaxToken) {
        PredicateDeclarationSyntaxBuilder::push_semicolon_token(self, token);
    }
}

impl PredicateSyntaxSink for TraitPredicateMemberDeclarationSyntaxBuilder {
    fn push_equals_token(&mut self, token: SyntaxToken) {
        TraitPredicateMemberDeclarationSyntaxBuilder::push_equals_token(self, token);
    }

    fn push_expression(&mut self, expression: ExpressionSyntax) {
        TraitPredicateMemberDeclarationSyntaxBuilder::push_expression(self, expression);
    }

    fn push_semicolon_token(&mut self, token: SyntaxToken) {
        TraitPredicateMemberDeclarationSyntaxBuilder::push_semicolon_token(self, token);
    }
}

impl SeparatedListSyntaxSink<PredicateParameterSyntax> for PredicateParameterListSyntaxBuilder {
    fn push_item(&mut self, item: PredicateParameterSyntax) {
        PredicateParameterListSyntaxBuilder::push_predicate_parameter(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        PredicateParameterListSyntaxBuilder::push_separator_token(self, separator);
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::TextRange;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::{assert_missing_semicolon_diagnostic, marker_offset};

    #[test]
    fn parser_parses_predicate_declarations_after_source_unit_modules() {
        let source = "module main; public trusted predicate positive<T, const N: Int>(value: T) = value > 0;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.predicate_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one predicate declaration: {declarations:?}");
        };

        let parameters = declaration
            .predicate_parameter_list()
            .predicate_parameters()
            .collect::<Vec<_>>();

        let [parameter] = parameters.as_slice() else {
            panic!("expected one predicate parameter: {parameters:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            declaration.full_text(),
            "public trusted predicate positive<T, const N: Int>(value: T) = value > 0;"
        );

        assert_eq!(
            declaration
                .predicate_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            declaration
                .predicate_modifiers()
                .trusted_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::TrustedKeyword)
        );

        let generic_parameter_list = match declaration.generic_parameter_list() {
            Some(list) => list,
            None => panic!("expected generic parameter list"),
        };

        assert_eq!(generic_parameter_list.full_text(), "<T, const N: Int>");
        assert_eq!(generic_parameter_list.generic_type_parameters().count(), 1);
        assert_eq!(generic_parameter_list.generic_const_parameters().count(), 1);

        assert_eq!(
            parameter.identifier_token().kind(),
            SyntaxKind::IdentifierToken
        );

        assert_eq!(parameter.type_expression().full_text(), "T");
        assert_eq!(parameter.skipped_syntax().count(), 0);
        assert!(declaration.equals_token().is_some());

        assert_eq!(
            declaration
                .expression()
                .map(|expression| expression.full_text()),
            Some(String::from("value > 0"))
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_semicolon_tail_predicate_declarations_inside_block_modules() {
        let source = "module main { trusted predicate opaque(); }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let modules = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [module] = modules.as_slice() else {
            panic!("expected one block module declaration: {modules:?}");
        };

        let declarations = module
            .module_body()
            .predicate_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one predicate declaration: {declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "trusted predicate opaque(); ");

        assert!(declaration.equals_token().is_none());
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_predicate_declarations_inside_type_bodies() {
        let source = concat!(
            "module main; ",
            "struct Point { predicate valid(value: Int); } ",
            "union Shape { predicate drawable(); }"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let struct_declarations = source_unit.struct_declarations().collect::<Vec<_>>();
        let union_declarations = source_unit.union_declarations().collect::<Vec<_>>();

        let [struct_declaration] = struct_declarations.as_slice() else {
            panic!("expected one struct declaration: {struct_declarations:?}");
        };

        let [union_declaration] = union_declarations.as_slice() else {
            panic!("expected one union declaration: {union_declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            struct_declaration
                .struct_body()
                .unwrap_or_else(|| panic!("test struct must have a body"))
                .predicate_declarations()
                .count(),
            1
        );

        assert_eq!(
            union_declaration
                .union_body()
                .predicate_declarations()
                .count(),
            1
        );

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_trait_predicate_member_declarations() {
        let source = concat!(
            "module main; ",
            "trait Valid { ",
            "trusted predicate ready(); ",
            "predicate positive(value: Int) = value > 0; ",
            "func check(); ",
            "}"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let body = declaration.trait_body();

        let predicates = body
            .trait_predicate_member_declarations()
            .collect::<Vec<_>>();

        let callables = body
            .trait_callable_member_declarations()
            .collect::<Vec<_>>();

        let [required_predicate, defaulted_predicate] = predicates.as_slice() else {
            panic!("expected two trait predicate members: {predicates:?}");
        };

        let [callable] = callables.as_slice() else {
            panic!("expected one trait callable member: {callables:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            required_predicate.full_text(),
            "trusted predicate ready(); "
        );

        assert_eq!(
            required_predicate
                .trait_predicate_member_modifiers()
                .trusted_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::TrustedKeyword)
        );

        assert!(required_predicate.equals_token().is_none());

        assert_eq!(
            defaulted_predicate.full_text(),
            "predicate positive(value: Int) = value > 0; "
        );

        assert!(defaulted_predicate.equals_token().is_some());

        assert_eq!(
            defaulted_predicate
                .expression()
                .map(|expression| expression.full_text()),
            Some(String::from("value > 0"))
        );

        assert_eq!(callable.full_text(), "func check(); ");

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_reports_missing_trait_predicate_member_semicolon_before_following_member() {
        let source = "module main; trait Valid { predicate ready()\nfunc check(); }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();
        let insertion = marker_offset(source, "func");

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let body = declaration.trait_body();

        let predicates = body
            .trait_predicate_member_declarations()
            .collect::<Vec<_>>();

        let callables = body
            .trait_callable_member_declarations()
            .collect::<Vec<_>>();

        let [predicate] = predicates.as_slice() else {
            panic!("expected one trait predicate member: {predicates:?}");
        };

        let [callable] = callables.as_slice() else {
            panic!("expected one trait callable member: {callables:?}");
        };

        let semicolon_token = predicate.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(predicate.full_text(), "predicate ready()\n");
        assert_eq!(callable.full_text(), "func check(); ");

        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::FuncKeyword,
            "func",
            &[DiagnosticKind::SyntaxExpectedToken],
        );
    }

    #[test]
    fn parser_reports_missing_predicate_semicolon_before_following_declaration_start() {
        let source = "module main; predicate ready()\nfunc main() {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let predicates = source_unit.predicate_declarations().collect::<Vec<_>>();
        let functions = source_unit.function_declarations().collect::<Vec<_>>();

        let insertion = marker_offset(source, "func");

        let [predicate] = predicates.as_slice() else {
            panic!("expected one predicate declaration: {predicates:?}");
        };

        let [function] = functions.as_slice() else {
            panic!("expected one function declaration: {functions:?}");
        };

        let semicolon_token = predicate.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(predicate.full_text(), "predicate ready()\n");
        assert_eq!(function.full_text(), "func main() {}");
        assert!(source_unit.skipped_syntax().next().is_none());

        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.kind(), SyntaxKind::SemicolonToken);
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::FuncKeyword,
            "func",
            &[DiagnosticKind::SyntaxExpectedToken],
        );
    }

    #[test]
    fn parser_parses_predicate_callable_type_parameters() {
        let source = "module main; predicate accepts(callback: func(value: Int) -> Bool);";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let predicates = source_unit.predicate_declarations().collect::<Vec<_>>();

        let [predicate] = predicates.as_slice() else {
            panic!("expected one predicate declaration: {predicates:?}");
        };

        let parameters = predicate
            .predicate_parameter_list()
            .predicate_parameters()
            .collect::<Vec<_>>();

        let [parameter] = parameters.as_slice() else {
            panic!("expected one predicate parameter: {parameters:?}");
        };

        assert_eq!(source_unit.full_text(), source);

        assert_eq!(
            parameter.type_expression().full_text(),
            "func(value: Int) -> Bool"
        );

        assert!(result.diagnostics().is_empty());
    }
}
