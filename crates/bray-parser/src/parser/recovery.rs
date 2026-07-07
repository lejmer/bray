use bray_syntax::{
    BlockModuleDeclarationSyntaxBuilder, CallableBodyBlockExpressionSyntaxBuilder,
    CallableContractDeclarationSyntaxBuilder, CallableResultClauseSyntaxBuilder,
    DirectiveArgumentListSyntaxBuilder, ExportDeclarationSyntaxBuilder,
    FunctionDeclarationSyntaxBuilder, FunctionDirectivesSyntaxBuilder, IdentifierListSyntaxBuilder,
    ImplementationSubjectSyntaxBuilder, InherentImplementationBodySyntaxBuilder,
    InherentImplementationDeclarationSyntaxBuilder, ModuleBodySyntaxBuilder,
    ModuleDirectivesSyntaxBuilder, NamedTraitImplementationDeclarationSyntaxBuilder,
    ParameterListSyntaxBuilder, ParameterSyntaxBuilder, PredicateDeclarationSyntaxBuilder,
    PredicateParameterListSyntaxBuilder, PredicateParameterSyntaxBuilder,
    SourceUnitModuleDeclarationSyntaxBuilder, SourceUnitSyntaxBuilder, StructBodySyntaxBuilder,
    StructDeclarationSyntaxBuilder, SyntaxKind, SyntaxToken, TargetDirectiveSyntaxBuilder,
    TraitApplicationSyntaxBuilder, TraitBodySyntaxBuilder, TraitDeclarationSyntaxBuilder,
    TraitImplementationBodySyntaxBuilder, TypeDirectivesSyntaxBuilder, UnionBodySyntaxBuilder,
    UnionDeclarationSyntaxBuilder, UnnamedTraitImplementationDeclarationSyntaxBuilder,
    UsingDeclarationSyntaxBuilder,
};

use crate::cursor::RecoverySet;

use super::state::Parser;

pub(super) trait RecoverySyntaxSink {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>);
}

pub(super) trait BracedBodySyntaxSink: RecoverySyntaxSink {
    fn push_open_brace_token(&mut self, token: SyntaxToken);

    fn push_close_brace_token(&mut self, token: SyntaxToken);
}

impl Parser {
    pub(super) fn recover_until(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        stop_kinds: &[SyntaxKind],
    ) -> bool {
        self.recover_until_set(builder, RecoverySet::new(stop_kinds))
    }

    pub(super) fn recover_until_set(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        recovery_set: RecoverySet<'_>,
    ) -> bool {
        let skipped_tokens = self.skip_until(recovery_set);
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

    pub(super) fn recover_current_and_until(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        stop_kinds: &[SyntaxKind],
    ) -> bool {
        let skipped_tokens = self.skip_current_and_until(RecoverySet::new(stop_kinds));
        let skipped_any = !skipped_tokens.is_empty();

        builder.push_skipped_tokens(skipped_tokens);

        skipped_any
    }

    pub(super) fn recover_current_and_until_predicate(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        mut at_stop: impl FnMut(&mut Parser) -> bool,
    ) -> bool {
        let mut skipped_tokens = Vec::new();

        if !self.at(SyntaxKind::EndOfFileToken) {
            skipped_tokens.push(self.consume());
        }

        while !self.at(SyntaxKind::EndOfFileToken) && !at_stop(self) {
            skipped_tokens.push(self.consume());
        }

        let skipped_any = !skipped_tokens.is_empty();

        self.record_skipped_syntax_for_tokens(&skipped_tokens);
        builder.push_skipped_tokens(skipped_tokens);

        skipped_any
    }

    pub(super) fn recover_current_and_until_balanced_close_paren_or_predicate(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        mut at_stop: impl FnMut(&mut Parser) -> bool,
    ) -> bool {
        let mut skipped_tokens = Vec::new();
        let mut paren_depth = 0usize;

        if !self.at(SyntaxKind::EndOfFileToken) {
            let skipped_token = self.consume();

            if skipped_token.kind() == SyntaxKind::OpenParenToken {
                paren_depth += 1;
            }

            skipped_tokens.push(skipped_token);
        }

        while !self.at(SyntaxKind::EndOfFileToken) && (paren_depth > 0 || !at_stop(self)) {
            let skipped_token = self.consume();

            match skipped_token.kind() {
                SyntaxKind::OpenParenToken => paren_depth += 1,
                SyntaxKind::CloseParenToken => paren_depth = paren_depth.saturating_sub(1),
                _ => {}
            }

            skipped_tokens.push(skipped_token);
        }

        let skipped_any = !skipped_tokens.is_empty();

        self.record_skipped_syntax_for_tokens(&skipped_tokens);
        builder.push_skipped_tokens(skipped_tokens);

        skipped_any
    }

    pub(super) fn recover_until_balanced_close_brace_or_recovery_set(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        recovery_set: RecoverySet<'_>,
    ) -> bool {
        let skipped_tokens = self.skip_until_balanced_close_brace_or_recovery(recovery_set);
        let skipped_any = !skipped_tokens.is_empty();

        builder.push_skipped_tokens(skipped_tokens);

        skipped_any
    }

    pub(super) fn recover_until_balanced_close_paren(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        stop_kinds: &[SyntaxKind],
    ) {
        let skipped_tokens = self.skip_until_balanced_close_paren(RecoverySet::new(stop_kinds));

        builder.push_skipped_tokens(skipped_tokens);
    }

    pub(super) fn parse_skipped_braced_body_tokens(
        &mut self,
        builder: &mut impl BracedBodySyntaxSink,
        mut at_missing_body_boundary: impl FnMut(&mut Parser) -> bool,
    ) {
        let open_brace_token = self.expect(SyntaxKind::OpenBraceToken);
        let open_brace_missing = open_brace_token.is_missing();

        builder.push_open_brace_token(open_brace_token);

        if open_brace_missing && at_missing_body_boundary(self) {
            builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));

            return;
        }

        self.recover_until_balanced_close_brace_or_recovery_set(builder, RecoverySet::new(&[]));

        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));
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

impl RecoverySyntaxSink for UsingDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        UsingDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ExportDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ExportDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ModuleDirectivesSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ModuleDirectivesSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for FunctionDirectivesSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        FunctionDirectivesSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for FunctionDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        FunctionDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for PredicateDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        PredicateDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for PredicateParameterListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        PredicateParameterListSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for PredicateParameterSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        PredicateParameterSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for CallableContractDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        CallableContractDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitBodySyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitBodySyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ImplementationSubjectSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ImplementationSubjectSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitApplicationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitApplicationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for InherentImplementationDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        InherentImplementationDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for InherentImplementationBodySyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        InherentImplementationBodySyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for UnnamedTraitImplementationDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        UnnamedTraitImplementationDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for NamedTraitImplementationDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        NamedTraitImplementationDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitImplementationBodySyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitImplementationBodySyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TypeDirectivesSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TypeDirectivesSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for StructDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        StructDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for StructBodySyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        StructBodySyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for UnionDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        UnionDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for UnionBodySyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        UnionBodySyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ParameterListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ParameterListSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ParameterSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ParameterSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for CallableResultClauseSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        CallableResultClauseSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for CallableBodyBlockExpressionSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        CallableBodyBlockExpressionSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TargetDirectiveSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TargetDirectiveSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for DirectiveArgumentListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        DirectiveArgumentListSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for IdentifierListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        IdentifierListSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl BracedBodySyntaxSink for TraitBodySyntaxBuilder {
    fn push_open_brace_token(&mut self, token: SyntaxToken) {
        TraitBodySyntaxBuilder::push_open_brace_token(self, token);
    }

    fn push_close_brace_token(&mut self, token: SyntaxToken) {
        TraitBodySyntaxBuilder::push_close_brace_token(self, token);
    }
}

impl BracedBodySyntaxSink for InherentImplementationBodySyntaxBuilder {
    fn push_open_brace_token(&mut self, token: SyntaxToken) {
        InherentImplementationBodySyntaxBuilder::push_open_brace_token(self, token);
    }

    fn push_close_brace_token(&mut self, token: SyntaxToken) {
        InherentImplementationBodySyntaxBuilder::push_close_brace_token(self, token);
    }
}

impl BracedBodySyntaxSink for TraitImplementationBodySyntaxBuilder {
    fn push_open_brace_token(&mut self, token: SyntaxToken) {
        TraitImplementationBodySyntaxBuilder::push_open_brace_token(self, token);
    }

    fn push_close_brace_token(&mut self, token: SyntaxToken) {
        TraitImplementationBodySyntaxBuilder::push_close_brace_token(self, token);
    }
}

impl BracedBodySyntaxSink for StructBodySyntaxBuilder {
    fn push_open_brace_token(&mut self, token: SyntaxToken) {
        StructBodySyntaxBuilder::push_open_brace_token(self, token);
    }

    fn push_close_brace_token(&mut self, token: SyntaxToken) {
        StructBodySyntaxBuilder::push_close_brace_token(self, token);
    }
}

impl BracedBodySyntaxSink for UnionBodySyntaxBuilder {
    fn push_open_brace_token(&mut self, token: SyntaxToken) {
        UnionBodySyntaxBuilder::push_open_brace_token(self, token);
    }

    fn push_close_brace_token(&mut self, token: SyntaxToken) {
        UnionBodySyntaxBuilder::push_close_brace_token(self, token);
    }
}

impl BracedBodySyntaxSink for CallableBodyBlockExpressionSyntaxBuilder {
    fn push_open_brace_token(&mut self, token: SyntaxToken) {
        CallableBodyBlockExpressionSyntaxBuilder::push_open_brace_token(self, token);
    }

    fn push_close_brace_token(&mut self, token: SyntaxToken) {
        CallableBodyBlockExpressionSyntaxBuilder::push_close_brace_token(self, token);
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
