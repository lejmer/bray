use bray_syntax::{
    BlockModuleDeclarationSyntax, BlockModuleDeclarationSyntaxBuilder, ModuleBodySyntax,
    ModuleModifiersSyntax, PathSyntax, SourceUnitModuleDeclarationSyntax,
    SourceUnitModuleDeclarationSyntaxBuilder, SourceUnitSyntaxBuilder, SyntaxKind, SyntaxToken,
};

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

impl Parser {
    pub(super) fn parse_source_unit_module_declaration(
        &mut self,
    ) -> SourceUnitModuleDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SourceUnitModuleDeclarationSyntax::builder(self.syntax_source(), start);

        self.parse_module_declaration_header(&mut builder);
        self.recover_until(
            &mut builder,
            &[SyntaxKind::SemicolonToken, SyntaxKind::EndOfFileToken],
        );
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
        self.recover_until(builder, &MODULE_HEADER_START_KINDS);

        builder.push_module_modifiers(self.parse_module_modifiers());
        builder.push_module_keyword(self.expect(SyntaxKind::ModuleKeyword));
        builder.push_module_path(self.parse_module_path());
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
        self.recover_until_balanced_close_brace(&mut builder);
        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));

        builder.build()
    }

    pub(super) fn parse_module_items(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        terminators: &[SyntaxKind],
    ) {
        while !self.at_any(terminators) && !self.at(SyntaxKind::EndOfFileToken) {
            self.parse_module_item(builder, terminators);
        }
    }

    fn parse_module_item(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        terminators: &[SyntaxKind],
    ) {
        let start = self.peek().start();

        if self.recover_until(builder, terminators) || self.peek().start() != start {
            return;
        }

        self.recover_current_token(builder);
    }

    pub(super) fn should_parse_block_module_declaration(&mut self) -> bool {
        if !self.at_any(&MODULE_DECLARATION_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.skip_module_directives_for_scan();
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

    fn skip_module_directives_for_scan(&mut self) {
        while !self.at_any(&MODULE_HEADER_START_KINDS) && !self.at(SyntaxKind::EndOfFileToken) {
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
    fn push_module_modifiers(&mut self, modifiers: ModuleModifiersSyntax);

    fn push_module_keyword(&mut self, token: SyntaxToken);

    fn push_module_path(&mut self, path: PathSyntax);
}

impl ModuleDeclarationSyntaxSink for SourceUnitModuleDeclarationSyntaxBuilder {
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

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::parse_diagnostic_kinds;

    #[test]
    fn parser_parses_source_unit_module_declarations() {
        let sources = source_store(["trusted public module main.core; func main() {}"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];

        let declaration = match source_unit.source_unit_module_declaration() {
            Some(declaration) => declaration,
            None => panic!("expected source-unit module declaration"),
        };

        let modifiers = declaration.module_modifiers();
        let path = declaration.module_path();
        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        assert_eq!(
            source_unit.full_text(),
            "trusted public module main.core; func main() {}"
        );

        assert_eq!(declaration.full_text(), "trusted public module main.core; ");

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

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(skipped.full_text(), "func main() {}");

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }

    #[test]
    fn parser_parses_block_module_declarations_and_skips_body_items() {
        let sources = source_store(["internal module main { func run() {} } module extra {}"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.block_module_declarations().collect::<Vec<_>>();

        let [first, second] = declarations.as_slice() else {
            panic!("expected two block module declarations: {declarations:?}");
        };

        assert_eq!(
            source_unit.full_text(),
            "internal module main { func run() {} } module extra {}"
        );

        assert!(source_unit.source_unit_module_declaration().is_none());

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
}
