use bray_syntax::{
    BlockModuleDeclarationSyntax, BlockModuleDeclarationSyntaxBuilder, DirectiveArgumentListSyntax,
    ExportDeclarationSyntax, LinkDirectiveSyntax, ModuleBodySyntax, ModuleBodySyntaxBuilder,
    ModuleDirectivesSyntax, ModuleDirectivesSyntaxBuilder, ModuleModifiersSyntax, PathSyntax,
    SourceUnitModuleDeclarationSyntax, SourceUnitModuleDeclarationSyntaxBuilder,
    SourceUnitSyntaxBuilder, SyntaxKind, SyntaxToken, TargetDirectiveSyntax, TestDirectiveSyntax,
    UsingDeclarationSyntax,
};

use crate::cursor::RecoverySet;

use super::recovery::RecoverySyntaxSink;
use super::state::Parser;

const MODULE_HEADER_START_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
];

const MODULE_DECLARATION_START_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
];

const TOP_LEVEL_BLOCK_MODULE_RECOVERY_KINDS: [SyntaxKind; 6] = [
    SyntaxKind::AtToken,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
    SyntaxKind::EndOfFileToken,
];

const DIRECTIVE_ARGUMENT_RECOVERY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::AtToken,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::ModuleKeyword,
];

const PARSED_MODULE_ITEM_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::UsingKeyword,
    SyntaxKind::ExportKeyword,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const MODULE_ITEM_DECLARATION_TERMINATOR_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const MODULE_ITEM_START_KINDS: [SyntaxKind; 18] = [
    SyntaxKind::UsingKeyword,
    SyntaxKind::ExportKeyword,
    SyntaxKind::AtToken,
    SyntaxKind::PublicKeyword,
    SyntaxKind::InternalKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::ExternKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::ConstKeyword,
    SyntaxKind::CallableKeyword,
    SyntaxKind::FuncKeyword,
    SyntaxKind::StructKeyword,
    SyntaxKind::UnionKeyword,
    SyntaxKind::TraitKeyword,
    SyntaxKind::ImplKeyword,
    SyntaxKind::OverloadKeyword,
    SyntaxKind::PredicateKeyword,
    SyntaxKind::ModuleKeyword,
];

const TARGET_DIRECTIVE_NAME: &str = "target";
const TEST_DIRECTIVE_NAME: &str = "test";
const LINK_DIRECTIVE_NAME: &str = "link";

impl Parser {
    pub(super) fn parse_source_unit_module_declaration(
        &mut self,
    ) -> SourceUnitModuleDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SourceUnitModuleDeclarationSyntax::builder(self.syntax_source(), start);

        self.parse_module_declaration_header(&mut builder);
        self.recover_until_module_item_declaration_end(&mut builder);

        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    pub(super) fn parse_block_module_declarations(
        &mut self,
        builder: &mut SourceUnitSyntaxBuilder,
    ) {
        loop {
            if self.at(SyntaxKind::EndOfFileToken) {
                return;
            }

            if self.should_parse_block_module_declaration() {
                let declaration = self.parse_block_module_declaration();

                builder.push_block_module_declaration(declaration);
                continue;
            }

            if self.recover_until(builder, &TOP_LEVEL_BLOCK_MODULE_RECOVERY_KINDS) {
                continue;
            }

            self.recover_current_token(builder);
        }
    }

    fn parse_block_module_declaration(&mut self) -> BlockModuleDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = BlockModuleDeclarationSyntax::builder(self.syntax_source(), start);

        self.parse_module_declaration_header(&mut builder);
        builder.push_module_body(self.parse_module_body());

        builder.build()
    }

    fn parse_module_declaration_header(&mut self, builder: &mut impl ModuleDeclarationSyntaxSink) {
        self.recover_until(builder, &MODULE_DECLARATION_START_KINDS);

        builder.push_module_directives(self.parse_module_directives());
        self.recover_until(builder, &MODULE_HEADER_START_KINDS);

        builder.push_module_modifiers(self.parse_module_modifiers());
        builder.push_module_keyword(self.expect(SyntaxKind::ModuleKeyword));
        builder.push_module_path(self.parse_module_path());
    }

    fn parse_module_directives(&mut self) -> ModuleDirectivesSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ModuleDirectivesSyntax::builder(self.syntax_source(), start);

        while self.at(SyntaxKind::AtToken) {
            if self.at_directive_name(TARGET_DIRECTIVE_NAME) {
                builder.push_target_directive(self.parse_target_directive());
                continue;
            }

            if self.at_directive_name(TEST_DIRECTIVE_NAME) {
                builder.push_test_directive(self.parse_test_directive());
                continue;
            }

            if self.at_directive_name(LINK_DIRECTIVE_NAME) {
                builder.push_link_directive(self.parse_link_directive());
                continue;
            }

            self.recover_unknown_module_directive(&mut builder);
        }

        builder.build()
    }

    fn parse_target_directive(&mut self) -> TargetDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TargetDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        // TODO(parser): Parse target directive arguments once directive arguments are implemented.
        self.recover_until_balanced_close_paren(&mut builder, &DIRECTIVE_ARGUMENT_RECOVERY_KINDS);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_test_directive(&mut self) -> TestDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TestDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    fn parse_link_directive(&mut self) -> LinkDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = LinkDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));
        builder.push_directive_argument_list(self.parse_directive_argument_list());

        builder.build()
    }

    fn parse_directive_argument_list(&mut self) -> DirectiveArgumentListSyntax {
        let start = self.peek().full_range().start();
        let mut builder = DirectiveArgumentListSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        // TODO(parser): Parse directive argument items once directive arguments are implemented.
        self.recover_until_balanced_close_paren(&mut builder, &DIRECTIVE_ARGUMENT_RECOVERY_KINDS);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn recover_unknown_module_directive(&mut self, builder: &mut ModuleDirectivesSyntaxBuilder) {
        self.recover_current_and_until(builder, &MODULE_DECLARATION_START_KINDS);
    }

    fn at_directive_name(&mut self, name: &str) -> bool {
        if !self.at(SyntaxKind::AtToken) {
            return false;
        }

        let name_token = self.lookahead(1);

        name_token.kind() == SyntaxKind::IdentifierToken
            && self.token_text(&name_token) == Some(name)
    }

    fn parse_module_modifiers(&mut self) -> ModuleModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ModuleModifiersSyntax::builder(self.syntax_source(), start);

        if self.at(SyntaxKind::TrustedKeyword) {
            builder.push_trusted_token(self.parse_trusted_modifier());
        }

        if self.at(SyntaxKind::PublicKeyword) || self.at(SyntaxKind::InternalKeyword) {
            builder.push_visibility_token(self.parse_visibility_modifier());
        }

        builder.build()
    }

    fn parse_trusted_modifier(&mut self) -> SyntaxToken {
        self.expect(SyntaxKind::TrustedKeyword)
    }

    fn parse_visibility_modifier(&mut self) -> SyntaxToken {
        if self.at(SyntaxKind::PublicKeyword) {
            return self.expect(SyntaxKind::PublicKeyword);
        }

        self.expect(SyntaxKind::InternalKeyword)
    }

    fn parse_module_path(&mut self) -> PathSyntax {
        self.parse_path()
    }

    fn parse_module_body(&mut self) -> ModuleBodySyntax {
        let start = self.peek().full_range().start();
        let mut builder = ModuleBodySyntax::builder(self.syntax_source(), start);

        builder.push_open_brace_token(self.expect(SyntaxKind::OpenBraceToken));
        self.parse_module_items(&mut builder, &[SyntaxKind::CloseBraceToken]);
        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));

        builder.build()
    }

    pub(super) fn parse_module_items(
        &mut self,
        builder: &mut impl ModuleItemSyntaxSink,
        terminators: &[SyntaxKind],
    ) {
        while !self.at_any(terminators) && !self.at(SyntaxKind::EndOfFileToken) {
            self.parse_module_item(builder, terminators);
        }
    }

    fn parse_module_item(
        &mut self,
        builder: &mut impl ModuleItemSyntaxSink,
        terminators: &[SyntaxKind],
    ) {
        let start = self.peek().start();

        if self.at(SyntaxKind::UsingKeyword) {
            builder.push_using_declaration(self.parse_using_declaration());
            return;
        }

        if self.at(SyntaxKind::ExportKeyword) {
            builder.push_export_declaration(self.parse_export_declaration());
            return;
        }

        // TODO(parser): Parse remaining module-level declarations as they are implemented.
        if self.recover_until_module_item_boundary(builder, terminators)
            || self.peek().start() != start
        {
            return;
        }

        self.recover_current_token(builder);
    }

    fn parse_using_declaration(&mut self) -> UsingDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = UsingDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_using_keyword(self.expect(SyntaxKind::UsingKeyword));

        if self.at(SyntaxKind::InternalKeyword) {
            builder.push_internal_keyword(self.expect(SyntaxKind::InternalKeyword));
        }

        builder.push_path(self.parse_path());
        self.recover_until_module_item_declaration_end(&mut builder);

        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn parse_export_declaration(&mut self) -> ExportDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ExportDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_export_keyword(self.expect(SyntaxKind::ExportKeyword));

        builder.push_path(self.parse_path());
        self.recover_until_module_item_declaration_end(&mut builder);

        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn recover_until_module_item_boundary(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        terminators: &[SyntaxKind],
    ) -> bool {
        let recovery_set =
            RecoverySet::new(&PARSED_MODULE_ITEM_BOUNDARY_KINDS).with_additional(terminators);

        self.recover_until_balanced_close_brace_or_recovery_set(builder, recovery_set)
    }

    fn recover_until_module_item_declaration_end(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
    ) -> bool {
        let recovery_set = RecoverySet::new(&MODULE_ITEM_DECLARATION_TERMINATOR_KINDS)
            .with_additional(&MODULE_ITEM_START_KINDS);

        self.recover_until_set(builder, recovery_set)
    }

    pub(super) fn should_parse_block_module_declaration(&mut self) -> bool {
        if !self.at_any(&MODULE_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_module_directives_for_scan();
            scan.consume_module_modifiers_for_scan();

            if !scan.at(SyntaxKind::ModuleKeyword) {
                return false;
            }

            scan.consume();

            if !scan.consume_path_for_scan() {
                return false;
            }

            scan.at(SyntaxKind::OpenBraceToken)
        })
    }

    fn consume_module_directives_for_scan(&mut self) {
        while self.at(SyntaxKind::AtToken) {
            self.consume();

            if !self.at(SyntaxKind::IdentifierToken) {
                continue;
            }

            let name_token = self.consume();
            let directive_name = self.token_text(&name_token);

            if directive_name == Some(TARGET_DIRECTIVE_NAME)
                || directive_name == Some(LINK_DIRECTIVE_NAME)
            {
                self.consume_directive_argument_list_for_scan();
                continue;
            }

            if directive_name != Some(TEST_DIRECTIVE_NAME) {
                self.skip_unknown_module_directive_for_scan();
            }
        }
    }

    fn consume_directive_argument_list_for_scan(&mut self) {
        if self.consume_if(SyntaxKind::OpenParenToken).is_none() {
            return;
        }

        self.scan_until_balanced_close_paren(&MODULE_DECLARATION_START_KINDS);
        self.consume_if(SyntaxKind::CloseParenToken);
    }

    fn skip_unknown_module_directive_for_scan(&mut self) {
        if self.consume_if(SyntaxKind::OpenParenToken).is_some() {
            self.scan_until_balanced_close_paren(&MODULE_DECLARATION_START_KINDS);
            self.consume_if(SyntaxKind::CloseParenToken);

            return;
        }

        while !self.at_any(&MODULE_DECLARATION_START_KINDS) && !self.at(SyntaxKind::EndOfFileToken)
        {
            self.consume();
        }
    }

    fn consume_module_modifiers_for_scan(&mut self) {
        if self.at(SyntaxKind::TrustedKeyword) {
            self.consume();
        }

        if self.at(SyntaxKind::PublicKeyword) || self.at(SyntaxKind::InternalKeyword) {
            self.consume();
        }
    }

    fn consume_path_for_scan(&mut self) -> bool {
        if !self.at(SyntaxKind::IdentifierToken) {
            return false;
        }

        self.consume();

        while self.at(SyntaxKind::DotToken) {
            self.consume();

            if !self.at(SyntaxKind::IdentifierToken) {
                break;
            }

            self.consume();
        }

        true
    }
}

trait ModuleDeclarationSyntaxSink: RecoverySyntaxSink {
    fn push_module_directives(&mut self, directives: ModuleDirectivesSyntax);

    fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax);

    fn push_module_keyword(&mut self, token: SyntaxToken);

    fn push_module_path(&mut self, path: PathSyntax);
}

pub(super) trait ModuleItemSyntaxSink: RecoverySyntaxSink {
    fn push_using_declaration(&mut self, declaration: UsingDeclarationSyntax);

    fn push_export_declaration(&mut self, declaration: ExportDeclarationSyntax);
}

impl ModuleDeclarationSyntaxSink for SourceUnitModuleDeclarationSyntaxBuilder {
    fn push_module_directives(&mut self, directives: ModuleDirectivesSyntax) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_directives(self, directives);
    }

    fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_modifiers(self, modifiers);
    }

    fn push_module_keyword(&mut self, token: SyntaxToken) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_keyword(self, token);
    }

    fn push_module_path(&mut self, path: PathSyntax) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_module_path(self, path);
    }
}

impl ModuleDeclarationSyntaxSink for BlockModuleDeclarationSyntaxBuilder {
    fn push_module_directives(&mut self, directives: ModuleDirectivesSyntax) {
        BlockModuleDeclarationSyntaxBuilder::push_module_directives(self, directives);
    }

    fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax) {
        BlockModuleDeclarationSyntaxBuilder::push_module_modifiers(self, modifiers);
    }

    fn push_module_keyword(&mut self, token: SyntaxToken) {
        BlockModuleDeclarationSyntaxBuilder::push_module_keyword(self, token);
    }

    fn push_module_path(&mut self, path: PathSyntax) {
        BlockModuleDeclarationSyntaxBuilder::push_module_path(self, path);
    }
}

impl ModuleItemSyntaxSink for SourceUnitSyntaxBuilder {
    fn push_using_declaration(&mut self, declaration: UsingDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_using_declaration(self, declaration);
    }

    fn push_export_declaration(&mut self, declaration: ExportDeclarationSyntax) {
        SourceUnitSyntaxBuilder::push_export_declaration(self, declaration);
    }
}

impl ModuleItemSyntaxSink for ModuleBodySyntaxBuilder {
    fn push_using_declaration(&mut self, declaration: UsingDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_using_declaration(self, declaration);
    }

    fn push_export_declaration(&mut self, declaration: ExportDeclarationSyntax) {
        ModuleBodySyntaxBuilder::push_export_declaration(self, declaration);
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind, SeverityKind};
    use bray_source::{TextRange, TextSize};
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::SyntaxTreeResult;
    use crate::parser::parse_compilation_unit;
    use crate::test_support::parse_diagnostic_kinds;

    #[test]
    fn parser_parses_source_unit_module_declarations() {
        let sources = source_store(["trusted public module main.core;"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let modifiers = declaration.module_modifiers();
        let path = declaration.module_path();

        assert_eq!(source_unit.full_text(), "trusted public module main.core;");
        assert_eq!(declaration.full_text(), "trusted public module main.core;");

        assert_eq!(
            modifiers.trusted_token().map(|token| token.kind()),
            Some(SyntaxKind::TrustedKeyword)
        );

        assert_eq!(
            modifiers.visibility_token().map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(path.full_text(), "main.core");
        assert_eq!(path.identifier_tokens().count(), 2);
        assert_eq!(path.dot_tokens().count(), 1);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_reports_missing_source_unit_module_semicolon_before_module_item_start() {
        let source = "module main\nusing std.io;";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let using_declarations = source_unit.using_declarations().collect::<Vec<_>>();

        let insertion = TextSize::new(
            source
                .find("using")
                .expect("test source should contain using keyword")
                .try_into()
                .expect("test source offset should fit in TextSize"),
        );

        let [using_declaration] = using_declarations.as_slice() else {
            panic!("expected one using declaration: {using_declarations:?}");
        };

        let semicolon_token = declaration.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "module main\n");
        assert_eq!(using_declaration.full_text(), "using std.io;");
        assert!(source_unit.skipped_syntax().next().is_none());

        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.kind(), SyntaxKind::SemicolonToken);
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::UsingKeyword,
            "using",
            &[DiagnosticKind::SyntaxExpectedToken],
        );
    }

    #[test]
    fn parser_reports_missing_source_unit_module_semicolon_before_module_start() {
        let source = "module main\nmodule extra {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        let insertion = TextSize::new(
            source
                .find("module extra")
                .expect("test source should contain second module keyword")
                .try_into()
                .expect("test source offset should fit in TextSize"),
        );

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        let semicolon_token = declaration.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(declaration.full_text(), "module main\n");
        assert_eq!(skipped.full_text(), "module extra {}");
        assert_eq!(source_unit.block_module_declarations().count(), 0);

        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.kind(), SyntaxKind::SemicolonToken);
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::ModuleKeyword,
            "module",
            &[
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxSkippedSyntax,
            ],
        );
    }

    #[test]
    fn parser_parses_test_directives_on_source_unit_module_declarations() {
        let sources = source_store(["@test module main;"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let directives = declaration.module_directives();

        assert_eq!(source_unit.full_text(), "@test module main;");
        assert_eq!(directives.full_text(), "@test ");

        assert_eq!(directives.test_directives().count(), 1);
        assert_eq!(directives.target_directives().count(), 0);
        assert_eq!(directives.link_directives().count(), 0);

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_target_and_link_module_directives() {
        let sources = source_store(["@target(host) @link(\"m\") module main;"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let directives = declaration.module_directives();
        let targets = directives.target_directives().collect::<Vec<_>>();
        let links = directives.link_directives().collect::<Vec<_>>();

        let [target] = targets.as_slice() else {
            panic!("expected one target directive: {targets:?}");
        };

        let [link] = links.as_slice() else {
            panic!("expected one link directive: {links:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "@target(host) @link(\"m\") module main;"
        );

        assert_eq!(directives.full_text(), "@target(host) @link(\"m\") ");
        assert_eq!(target.full_text(), "@target(host) ");
        assert_eq!(target.skipped_syntax().count(), 1);
        assert_eq!(link.full_text(), "@link(\"m\") ");

        assert_eq!(link.directive_argument_list().skipped_syntax().count(), 1);

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    #[test]
    fn parser_recovers_unknown_module_directives_without_losing_later_directives() {
        let sources = source_store(["@unknown(foo) @test module main;"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let directives = declaration.module_directives();
        let skipped_syntax = directives.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(source_unit.full_text(), "@unknown(foo) @test module main;");
        assert_eq!(directives.full_text(), "@unknown(foo) @test ");
        assert_eq!(skipped.full_text(), "@unknown(foo) ");

        assert_eq!(directives.test_directives().count(), 1);

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    #[test]
    fn parser_parses_block_module_declarations_and_skips_body_items() {
        let sources =
            source_store(["@test internal module main { func run() {} } module extra {}"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [first, second] = declarations.as_slice() else {
            panic!("expected two block module declarations: {declarations:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "@test internal module main { func run() {} } module extra {}"
        );

        assert!(source_unit.source_unit_module_declaration().is_none());
        assert_eq!(first.module_directives().test_directives().count(), 1);

        assert_eq!(
            first
                .module_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::InternalKeyword)
        );

        assert_eq!(first.module_path().full_text(), "main ");
        assert_eq!(first.module_body().full_text(), "{ func run() {} } ");
        assert_eq!(first.module_body().skipped_syntax().count(), 1);
        assert_eq!(second.module_path().full_text(), "extra ");
        assert!(second.module_body().skipped_syntax().next().is_none());

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    #[test]
    fn parser_parses_using_and_export_declarations_after_source_unit_modules() {
        let sources = source_store(["module main; using internal core.io; export api;"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let using_declarations = source_unit.using_declarations().collect::<Vec<_>>();
        let export_declarations = source_unit.export_declarations().collect::<Vec<_>>();

        let [using_declaration] = using_declarations.as_slice() else {
            panic!("expected one using declaration: {using_declarations:?}");
        };

        let [export_declaration] = export_declarations.as_slice() else {
            panic!("expected one export declaration: {export_declarations:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "module main; using internal core.io; export api;"
        );

        assert_eq!(using_declaration.full_text(), "using internal core.io; ");

        assert_eq!(
            using_declaration
                .internal_keyword()
                .map(|token| token.kind()),
            Some(SyntaxKind::InternalKeyword)
        );

        assert_eq!(using_declaration.path().full_text(), "core.io");
        assert_eq!(using_declaration.path().identifier_tokens().count(), 2);
        assert_eq!(using_declaration.path().dot_tokens().count(), 1);
        assert_eq!(export_declaration.full_text(), "export api;");
        assert_eq!(export_declaration.path().full_text(), "api");
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_reports_missing_using_semicolon_before_following_declaration_start() {
        let source = "module main; using std.io\nfunc main() {}";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let using_declarations = source_unit.using_declarations().collect::<Vec<_>>();
        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        let insertion = TextSize::new(
            source
                .find("func")
                .expect("test source should contain function keyword")
                .try_into()
                .expect("test source offset should fit in TextSize"),
        );

        let [using_declaration] = using_declarations.as_slice() else {
            panic!("expected one using declaration: {using_declarations:?}");
        };

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        let semicolon_token = using_declaration.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(using_declaration.full_text(), "using std.io\n");
        assert!(using_declaration.skipped_syntax().next().is_none());
        assert_eq!(skipped.full_text(), "func main() {}");

        assert!(semicolon_token.is_missing());

        assert_eq!(semicolon_token.kind(), SyntaxKind::SemicolonToken);
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::FuncKeyword,
            "func",
            &[
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxSkippedSyntax,
            ],
        );
    }

    #[test]
    fn parser_parses_using_and_export_declarations_inside_block_modules() {
        let sources = source_store(["module main { using core; export api; }"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one block module declaration: {declarations:?}");
        };

        let body = declaration.module_body();
        let using_declarations = body.using_declarations().collect::<Vec<_>>();
        let export_declarations = body.export_declarations().collect::<Vec<_>>();

        let [using_declaration] = using_declarations.as_slice() else {
            panic!("expected one using declaration: {using_declarations:?}");
        };

        let [export_declaration] = export_declarations.as_slice() else {
            panic!("expected one export declaration: {export_declarations:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "module main { using core; export api; }"
        );

        assert_eq!(body.full_text(), "{ using core; export api; }");
        assert_eq!(using_declaration.full_text(), "using core; ");
        assert!(using_declaration.internal_keyword().is_none());
        assert_eq!(using_declaration.path().full_text(), "core");
        assert_eq!(export_declaration.full_text(), "export api; ");
        assert_eq!(export_declaration.path().full_text(), "api");

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_recovers_unimplemented_module_items_without_losing_later_items() {
        let sources = source_store(["module main { func run() {} using core; }"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one block module declaration: {declarations:?}");
        };

        let body = declaration.module_body();
        let skipped_syntax = body.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "module main { func run() {} using core; }"
        );

        assert_eq!(body.using_declarations().count(), 1);
        assert_eq!(body.export_declarations().count(), 0);

        assert_eq!(skipped.full_text(), "func run() {} ");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    fn assert_missing_semicolon_diagnostic(
        result: &SyntaxTreeResult,
        insertion: TextSize,
        actual_kind: SyntaxKind,
        actual_text: &str,
        expected_kinds: &[DiagnosticKind],
    ) {
        let expected_diagnostic = result
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.kind() == DiagnosticKind::SyntaxExpectedToken)
            .expect("expected missing semicolon diagnostic");

        assert_eq!(parse_diagnostic_kinds(result).as_slice(), expected_kinds);
        assert_eq!(expected_diagnostic.severity(), SeverityKind::Error);

        assert_eq!(
            expected_diagnostic.primary_span().map(|span| span.range()),
            Some(TextRange::empty(insertion))
        );

        assert_eq!(
            expected_diagnostic.args(),
            &[
                DiagnosticArg::expected_syntax_kind(SyntaxKind::SemicolonToken),
                DiagnosticArg::actual_syntax_kind(actual_kind),
                DiagnosticArg::token_text(actual_text),
            ]
        );
    }
}
