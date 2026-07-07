use bray_syntax::{
    ConstantDeclarationSyntax, ConstantDeclarationSyntaxBuilder, ConstantModifiersSyntax,
    SyntaxKind, SyntaxToken, TraitConstantMemberDeclarationSyntax,
    TraitConstantMemberDeclarationSyntaxBuilder, TraitImplementationConstantMemberDefinitionSyntax,
    TraitImplementationConstantMemberDefinitionSyntaxBuilder,
};

use super::module::MODULE_ITEM_START_KINDS;
use super::recovery::RecoverySyntaxSink;
use super::state::Parser;

const CONSTANT_DECLARATION_START_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ConstKeyword,
];

const CONSTANT_TYPE_BOUNDARY_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const CONSTANT_VALUE_BOUNDARY_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

// Identifier starts are omitted because placeholder expressions can also start
// with identifiers.
const CONSTANT_FOLLOWING_MEMBER_START_KINDS: [SyntaxKind; 17] = [
    SyntaxKind::AtToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::StaticKeyword,
    SyntaxKind::ConsumeKeyword,
    SyntaxKind::MutKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::FuncKeyword,
    SyntaxKind::ConstructKeyword,
    SyntaxKind::FinalizeKeyword,
    SyntaxKind::DestructKeyword,
    SyntaxKind::EnterKeyword,
    SyntaxKind::ExitKeyword,
    SyntaxKind::TypeKeyword,
    SyntaxKind::PredicateKeyword,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConstantInitializerPolicy {
    Optional,
    Required,
}

impl Parser {
    pub(super) fn parse_constant_declaration(&mut self) -> ConstantDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ConstantDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_constant_modifiers(self.parse_constant_modifiers());
        self.parse_constant_declaration_core(&mut builder, ConstantInitializerPolicy::Required);

        builder.build()
    }

    pub(super) fn parse_trait_constant_member_declaration(
        &mut self,
    ) -> TraitConstantMemberDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            TraitConstantMemberDeclarationSyntax::builder(self.syntax_source(), start);

        self.parse_constant_declaration_core(&mut builder, ConstantInitializerPolicy::Optional);

        builder.build()
    }

    pub(super) fn parse_trait_implementation_constant_member_definition(
        &mut self,
    ) -> TraitImplementationConstantMemberDefinitionSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            TraitImplementationConstantMemberDefinitionSyntax::builder(self.syntax_source(), start);

        self.parse_constant_declaration_core(&mut builder, ConstantInitializerPolicy::Required);

        builder.build()
    }

    fn parse_constant_declaration_core(
        &mut self,
        builder: &mut impl ConstantDeclarationSyntaxSink,
        initializer_policy: ConstantInitializerPolicy,
    ) {
        builder.push_const_keyword(self.expect(SyntaxKind::ConstKeyword));
        builder.push_identifier_token(self.parse_identifier());
        builder.push_colon_token(self.expect(SyntaxKind::ColonToken));

        // TODO(parser): Parse constant type expressions once expression parsing is implemented.
        self.recover_constant_type_expression(builder);

        self.parse_constant_initializer(builder, initializer_policy);
        self.recover_until_constant_declaration_end(builder);
        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));
    }

    fn parse_constant_modifiers(&mut self) -> ConstantModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ConstantModifiersSyntax::builder(self.syntax_source(), start);

        if self.at_visibility_modifier() {
            builder.push_visibility_token(self.parse_visibility_modifier());
        }

        builder.build()
    }

    fn parse_constant_initializer(
        &mut self,
        builder: &mut impl ConstantDeclarationSyntaxSink,
        initializer_policy: ConstantInitializerPolicy,
    ) {
        match initializer_policy {
            ConstantInitializerPolicy::Required => {
                builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
            }
            ConstantInitializerPolicy::Optional => {
                if !self.at(SyntaxKind::EqualsToken) {
                    return;
                }

                builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
            }
        }

        // TODO(parser): Parse constant value expressions once expression parsing is implemented.
        self.recover_constant_value_expression(builder);
    }

    fn recover_constant_type_expression(
        &mut self,
        builder: &mut impl ConstantDeclarationSyntaxSink,
    ) {
        if self.at_constant_type_boundary() {
            return;
        }

        self.recover_current_and_until_predicate(builder, Parser::at_constant_type_boundary);
    }

    fn recover_constant_value_expression(
        &mut self,
        builder: &mut impl ConstantDeclarationSyntaxSink,
    ) {
        if self.at_constant_value_boundary() {
            return;
        }

        self.recover_current_and_until_predicate(builder, Parser::at_constant_value_boundary);
    }

    fn recover_until_constant_declaration_end(
        &mut self,
        builder: &mut impl ConstantDeclarationSyntaxSink,
    ) {
        self.recover_until_predicate(builder, Parser::at_constant_declaration_end);
    }

    fn at_constant_type_boundary(&mut self) -> bool {
        self.at_any(&CONSTANT_TYPE_BOUNDARY_KINDS) || self.at_constant_following_item_start()
    }

    fn at_constant_value_boundary(&mut self) -> bool {
        self.at_any(&CONSTANT_VALUE_BOUNDARY_KINDS) || self.at_constant_following_item_start()
    }

    fn at_constant_declaration_end(&mut self) -> bool {
        self.at_any(&CONSTANT_VALUE_BOUNDARY_KINDS) || self.at_constant_following_item_start()
    }

    fn at_constant_following_item_start(&mut self) -> bool {
        self.at_any(&MODULE_ITEM_START_KINDS) || self.at_any(&CONSTANT_FOLLOWING_MEMBER_START_KINDS)
    }

    pub(super) fn should_parse_constant_declaration(&mut self) -> bool {
        if !self.at_any(&CONSTANT_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            if scan.at_visibility_modifier() {
                scan.consume();
            }

            scan.at(SyntaxKind::ConstKeyword)
        })
    }

    pub(super) fn should_parse_trait_constant_member_declaration(&mut self) -> bool {
        self.at(SyntaxKind::ConstKeyword)
    }

    pub(super) fn should_parse_trait_implementation_constant_member_definition(&mut self) -> bool {
        self.at(SyntaxKind::ConstKeyword)
    }
}

trait ConstantDeclarationSyntaxSink: RecoverySyntaxSink {
    fn push_const_keyword(&mut self, token: SyntaxToken);

    fn push_identifier_token(&mut self, token: SyntaxToken);

    fn push_colon_token(&mut self, token: SyntaxToken);

    fn push_equals_token(&mut self, token: SyntaxToken);

    fn push_semicolon_token(&mut self, token: SyntaxToken);
}

impl ConstantDeclarationSyntaxSink for ConstantDeclarationSyntaxBuilder {
    fn push_const_keyword(&mut self, token: SyntaxToken) {
        ConstantDeclarationSyntaxBuilder::push_const_keyword(self, token);
    }

    fn push_identifier_token(&mut self, token: SyntaxToken) {
        ConstantDeclarationSyntaxBuilder::push_identifier_token(self, token);
    }

    fn push_colon_token(&mut self, token: SyntaxToken) {
        ConstantDeclarationSyntaxBuilder::push_colon_token(self, token);
    }

    fn push_equals_token(&mut self, token: SyntaxToken) {
        ConstantDeclarationSyntaxBuilder::push_equals_token(self, token);
    }

    fn push_semicolon_token(&mut self, token: SyntaxToken) {
        ConstantDeclarationSyntaxBuilder::push_semicolon_token(self, token);
    }
}

impl ConstantDeclarationSyntaxSink for TraitConstantMemberDeclarationSyntaxBuilder {
    fn push_const_keyword(&mut self, token: SyntaxToken) {
        TraitConstantMemberDeclarationSyntaxBuilder::push_const_keyword(self, token);
    }

    fn push_identifier_token(&mut self, token: SyntaxToken) {
        TraitConstantMemberDeclarationSyntaxBuilder::push_identifier_token(self, token);
    }

    fn push_colon_token(&mut self, token: SyntaxToken) {
        TraitConstantMemberDeclarationSyntaxBuilder::push_colon_token(self, token);
    }

    fn push_equals_token(&mut self, token: SyntaxToken) {
        TraitConstantMemberDeclarationSyntaxBuilder::push_equals_token(self, token);
    }

    fn push_semicolon_token(&mut self, token: SyntaxToken) {
        TraitConstantMemberDeclarationSyntaxBuilder::push_semicolon_token(self, token);
    }
}

impl ConstantDeclarationSyntaxSink for TraitImplementationConstantMemberDefinitionSyntaxBuilder {
    fn push_const_keyword(&mut self, token: SyntaxToken) {
        TraitImplementationConstantMemberDefinitionSyntaxBuilder::push_const_keyword(self, token);
    }

    fn push_identifier_token(&mut self, token: SyntaxToken) {
        TraitImplementationConstantMemberDefinitionSyntaxBuilder::push_identifier_token(
            self, token,
        );
    }

    fn push_colon_token(&mut self, token: SyntaxToken) {
        TraitImplementationConstantMemberDefinitionSyntaxBuilder::push_colon_token(self, token);
    }

    fn push_equals_token(&mut self, token: SyntaxToken) {
        TraitImplementationConstantMemberDefinitionSyntaxBuilder::push_equals_token(self, token);
    }

    fn push_semicolon_token(&mut self, token: SyntaxToken) {
        TraitImplementationConstantMemberDefinitionSyntaxBuilder::push_semicolon_token(self, token);
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{TextRange, TextSize};
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::{
        assert_missing_semicolon_diagnostic, marker_offset, parse_diagnostic_kinds, source,
    };

    use super::super::state::Parser;

    // TODO(parser): Update this when constant type and value expressions are parsed.
    #[test]
    fn parser_parses_constant_declarations_after_source_unit_modules() {
        let source = "module main; public const Answer: Int = 42;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.constant_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one constant declaration: {declarations:?}");
        };

        let skipped_syntax = declaration.skipped_syntax().collect::<Vec<_>>();

        let [type_expression, value_expression] = skipped_syntax.as_slice() else {
            panic!("expected type and value expressions as skipped syntax: {skipped_syntax:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "public const Answer: Int = 42;");

        assert_eq!(
            declaration
                .constant_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(type_expression.full_text(), "Int ");
        assert_eq!(value_expression.full_text(), "42");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    // TODO(parser): Update this when constant type and value expressions are parsed.
    #[test]
    fn parser_parses_constant_declarations_inside_block_modules() {
        let source = "module main { const Answer: Int = 42; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let modules = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [module] = modules.as_slice() else {
            panic!("expected one block module declaration: {modules:?}");
        };

        let body = module.module_body();
        let declarations = body.constant_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one constant declaration: {declarations:?}");
        };

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(body.full_text(), "{ const Answer: Int = 42; }");
        assert_eq!(declaration.full_text(), "const Answer: Int = 42; ");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    // TODO(parser): Update this when constant type and value expressions are parsed.
    #[test]
    fn parser_reports_missing_constant_semicolon_before_following_item_start() {
        let source = "module main; const Answer: Int = 42\nfunc main() {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.constant_declarations().collect::<Vec<_>>();
        let functions = source_unit.function_declarations().collect::<Vec<_>>();

        let insertion = marker_offset(source, "func");

        let [declaration] = declarations.as_slice() else {
            panic!("expected one constant declaration: {declarations:?}");
        };

        let [function] = functions.as_slice() else {
            panic!("expected one function declaration: {functions:?}");
        };

        let semicolon_token = declaration.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "const Answer: Int = 42\n");
        assert_eq!(function.full_text(), "func main() {}");

        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.kind(), SyntaxKind::SemicolonToken);
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::FuncKeyword,
            "func",
            &[
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxExpectedToken,
            ],
        );
    }

    #[test]
    fn parser_reports_missing_constant_initializer() {
        let source = "module main; const Answer: Int;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.constant_declarations().collect::<Vec<_>>();

        let insertion = match marker_offset(source, "Int;").checked_add(TextSize::new(3)) {
            Some(insertion) => insertion,
            None => panic!("expected constant initializer insertion marker to fit in TextSize"),
        };

        let [declaration] = declarations.as_slice() else {
            panic!("expected one constant declaration: {declarations:?}");
        };

        let equals_token = declaration.equals_token();

        assert_eq!(source_unit.full_text(), source);
        assert!(equals_token.is_missing());
        assert_eq!(equals_token.kind(), SyntaxKind::EqualsToken);
        assert_eq!(equals_token.range(), TextRange::empty(insertion));

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxExpectedToken
            ]
        );
    }

    #[test]
    fn parser_scan_ahead_recognizes_constants_without_consuming_tokens() {
        let sources = source_store(["public const Answer: Int = 42;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        assert!(parser.should_parse_constant_declaration());
        assert_eq!(parser.peek().kind(), SyntaxKind::PublicKeyword);

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }
}
