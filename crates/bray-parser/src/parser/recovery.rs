use bray_syntax::{
    BlockModuleDeclarationSyntaxBuilder, IdentifierListSyntaxBuilder, ModuleBodySyntaxBuilder,
    SourceUnitModuleDeclarationSyntaxBuilder, SourceUnitSyntaxBuilder, SyntaxKind, SyntaxToken,
};

use crate::cursor::RecoverySet;

use super::state::Parser;

pub(super) trait RecoverySyntaxSink {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>);
}

impl Parser {
    pub(super) fn recover_until(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        stop_kinds: &[SyntaxKind],
    ) -> bool {
        let skipped_tokens = self.skip_until(RecoverySet::new(stop_kinds));
        let skipped_any = !skipped_tokens.is_empty();

        builder.push_skipped_tokens(skipped_tokens);

        skipped_any
    }

    pub(super) fn recover_current_token(&mut self, builder: &mut impl RecoverySyntaxSink) {
        let Some(skipped_token) = self.skip_one() else {
            return;
        };

        builder.push_skipped_tokens(vec![skipped_token]);
    }

    pub(super) fn recover_until_balanced_close_brace(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
    ) {
        let skipped_tokens = self.skip_until_balanced_close_brace();

        builder.push_skipped_tokens(skipped_tokens);
    }
}

impl RecoverySyntaxSink for SourceUnitSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        SourceUnitSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for SourceUnitModuleDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        SourceUnitModuleDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for BlockModuleDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        BlockModuleDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ModuleBodySyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ModuleBodySyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for IdentifierListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        IdentifierListSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SourceUnitSyntax, SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use super::super::state::Parser;
    use crate::parser::parse_compilation_unit;
    use crate::test_support::{diagnostic_kinds, parse_diagnostic_kinds, source};

    #[test]
    fn parser_recovers_invalid_tokens_as_skipped_syntax() {
        let sources = source_store(["$"]);
        let result = parse_compilation_unit(&sources);
        let source_unit = &result.syntax_tree().root().source_units()[0];
        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        assert_eq!(
            parse_diagnostic_kinds(&result),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxSkippedSyntax,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof,
                DiagnosticKind::SyntaxUnexpectedEof
            ]
        );

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(skipped.full_text(), "$");
    }

    #[test]
    fn parser_recovery_helper_attaches_skipped_syntax_before_stop_token() {
        let sources = source_store(["main;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot.clone());
        let mut builder = SourceUnitSyntax::builder(snapshot.clone());

        parser.recover_until(&mut builder, &[SyntaxKind::SemicolonToken]);

        builder.push_token(parser.expect(SyntaxKind::SemicolonToken));
        builder.push_token(parser.expect(SyntaxKind::EndOfFileToken));

        let source_unit = builder.build();
        let diagnostics = parser.finish();
        let skipped_syntax = source_unit.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(skipped.full_text(), "main");
        assert_eq!(source_unit.full_text(), "main;");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxSkippedSyntax]
        );
    }
}
