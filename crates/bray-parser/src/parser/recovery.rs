use bray_syntax::{
    AccessExpressionSyntaxBuilder, ArgumentListSyntaxBuilder, ArgumentSyntaxBuilder,
    ArrayExpressionSyntaxBuilder, AssertionExpressionSyntaxBuilder,
    AsyncBlockExpressionSyntaxBuilder, AwaitExpressionSyntaxBuilder, BlockExpressionSyntaxBuilder,
    BlockItemSyntaxBuilder, BlockModuleDeclarationSyntaxBuilder,
    BooleanFoldExpressionSyntaxBuilder, BorrowExpressionSyntaxBuilder,
    BreakExpressionSyntaxBuilder, CallableContractDeclarationSyntaxBuilder,
    CallableDirectivesSyntaxBuilder, CallableOverloadDeclarationSyntaxBuilder,
    CallableResultClauseSyntaxBuilder, CatchExpressionSyntaxBuilder, ConditionalElseSyntaxBuilder,
    ConditionalExpressionSyntaxBuilder, ConstantDeclarationSyntaxBuilder,
    ContinueExpressionSyntaxBuilder, ConversionOperationSyntaxBuilder,
    DestructorMemberDeclarationSyntaxBuilder, DirectiveArgumentListSyntaxBuilder,
    ElementIndexOperationSyntaxBuilder, EnsuresClauseSyntaxBuilder, ExportDeclarationSyntaxBuilder,
    ExpressionSyntaxBuilder, FinalizerMemberDeclarationSyntaxBuilder, ForExpressionSyntaxBuilder,
    FunctionDeclarationSyntaxBuilder, FunctionDirectivesSyntaxBuilder,
    GeneralGeneratorExpressionSyntaxBuilder, GeneratorIterationExpressionSyntaxBuilder,
    GenericArgumentListSyntaxBuilder, GenericParameterListSyntaxBuilder,
    GroupedExpressionSyntaxBuilder, IdentifierListSyntaxBuilder, ImplementationBodySyntaxBuilder,
    ImplementationOverloadDeclarationSyntaxBuilder, ImplementationOverloadSubjectSyntaxBuilder,
    ImplementationSubjectSyntaxBuilder, ImplementationTypeMemberBindingSyntaxBuilder,
    InherentImplementationDeclarationSyntaxBuilder, IterationSourceSyntaxBuilder,
    LambdaExpressionSyntaxBuilder, LocalBindingDeclarationSyntaxBuilder,
    LoopExpressionSyntaxBuilder, MatchArmSyntaxBuilder, MatchBodySyntaxBuilder,
    MatchExpressionSyntaxBuilder, MatchSubjectSyntaxBuilder, MemberAccessOperationSyntaxBuilder,
    ModuleBodySyntaxBuilder, ModuleDirectivesSyntaxBuilder,
    NamedTraitImplementationDeclarationSyntaxBuilder, OverloadArmListSyntaxBuilder,
    PanicExpressionSyntaxBuilder, ParameterListSyntaxBuilder, ParameterSyntaxBuilder,
    PredicateDeclarationSyntaxBuilder, PredicateParameterListSyntaxBuilder,
    PredicateParameterSyntaxBuilder, PrimaryExpressionSyntaxBuilder, RequiresClauseSyntaxBuilder,
    ResultPropagationExpressionSyntaxBuilder, ReturnExpressionSyntaxBuilder,
    ScopeEnterMemberDeclarationSyntaxBuilder, ScopeExitMemberDeclarationSyntaxBuilder,
    SequencedExpressionSyntaxBuilder, SliceIndexOperationSyntaxBuilder,
    SourceUnitModuleDeclarationSyntaxBuilder, SourceUnitSyntaxBuilder,
    SpawnExpressionSyntaxBuilder, StructBodySyntaxBuilder, StructConstructionBodySyntaxBuilder,
    StructDeclarationSyntaxBuilder, StructFieldDeclarationSyntaxBuilder,
    StructFieldInitializerSyntaxBuilder, SyntaxKind, SyntaxToken, TraitApplicationSyntaxBuilder,
    TraitBodySyntaxBuilder, TraitCallableMemberDeclarationSyntaxBuilder,
    TraitConstantMemberDeclarationSyntaxBuilder, TraitDeclarationSyntaxBuilder,
    TraitDestructorRequirementDeclarationSyntaxBuilder,
    TraitFinalizerRequirementDeclarationSyntaxBuilder,
    TraitPredicateMemberDeclarationSyntaxBuilder, TraitQualifiedMemberOperationSyntaxBuilder,
    TraitScopeEnterRequirementDeclarationSyntaxBuilder,
    TraitScopeExitRequirementDeclarationSyntaxBuilder, TraitTypeMemberDeclarationSyntaxBuilder,
    TrustBoundaryExpressionSyntaxBuilder, TupleExpressionSyntaxBuilder,
    TypeCallableMemberDeclarationSyntaxBuilder, TypeConstructorMemberDeclarationSyntaxBuilder,
    TypeDirectivesSyntaxBuilder, TypeExpressionSyntaxBuilder, TypeFormArgumentListSyntaxBuilder,
    TypeFormConstructionExpressionSyntaxBuilder, UnionBodySyntaxBuilder,
    UnionDeclarationSyntaxBuilder, UnionPayloadFieldSyntaxBuilder,
    UnionVariantDeclarationSyntaxBuilder, UnionVariantPayloadSyntaxBuilder,
    UnnamedTraitImplementationDeclarationSyntaxBuilder, UsesClauseSyntaxBuilder,
    UsingDeclarationSyntaxBuilder, VariantDirectivesSyntaxBuilder, WhileExpressionSyntaxBuilder,
    WithClauseSyntaxBuilder, WithExpressionSyntaxBuilder, YieldExpressionSyntaxBuilder,
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

macro_rules! impl_recovery_syntax_sink {
    ($($builder:ident),+ $(,)?) => {
        $(
            impl RecoverySyntaxSink for $builder {
                fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
                    $builder::push_skipped_tokens(self, tokens);
                }
            }
        )+
    };
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

    pub(super) fn recover_until_predicate(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        mut at_stop: impl FnMut(&mut Parser) -> bool,
    ) -> bool {
        let mut skipped_tokens = Vec::new();

        while !self.at(SyntaxKind::EndOfFileToken) && !at_stop(self) {
            skipped_tokens.push(self.consume());
        }

        let skipped_any = !skipped_tokens.is_empty();

        builder.push_skipped_tokens(skipped_tokens);

        skipped_any
    }

    pub(super) fn recover_current_and_until_balanced_close_brace_or_recovery_set(
        &mut self,
        builder: &mut impl RecoverySyntaxSink,
        recovery_set: RecoverySet<'_>,
    ) -> bool {
        let skipped_tokens =
            self.skip_current_and_until_balanced_close_brace_or_recovery(recovery_set);

        let skipped_any = !skipped_tokens.is_empty();

        builder.push_skipped_tokens(skipped_tokens);

        skipped_any
    }

    pub(super) fn parse_braced_body_contents<Builder>(
        &mut self,
        builder: &mut Builder,
        mut at_missing_body_boundary: impl FnMut(&mut Parser) -> bool,
        mut parse_contents: impl FnMut(&mut Parser, &mut Builder),
    ) where
        Builder: BracedBodySyntaxSink,
    {
        let open_brace_token = self.expect(SyntaxKind::OpenBraceToken);
        let open_brace_missing = open_brace_token.is_missing();

        builder.push_open_brace_token(open_brace_token);

        if open_brace_missing && at_missing_body_boundary(self) {
            builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));

            return;
        }

        parse_contents(self, builder);

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

impl RecoverySyntaxSink for ConstantDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ConstantDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
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

impl RecoverySyntaxSink for CallableDirectivesSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        CallableDirectivesSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for GenericParameterListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        GenericParameterListSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for GenericArgumentListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        GenericArgumentListSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TypeFormArgumentListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TypeFormArgumentListSyntaxBuilder::push_skipped_tokens(self, tokens);
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

impl RecoverySyntaxSink for CallableOverloadDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        CallableOverloadDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ImplementationOverloadDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ImplementationOverloadDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ImplementationOverloadSubjectSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ImplementationOverloadSubjectSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for OverloadArmListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        OverloadArmListSyntaxBuilder::push_skipped_tokens(self, tokens);
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

impl RecoverySyntaxSink for ImplementationBodySyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ImplementationBodySyntaxBuilder::push_skipped_tokens(self, tokens);
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

impl RecoverySyntaxSink for ImplementationTypeMemberBindingSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ImplementationTypeMemberBindingSyntaxBuilder::push_skipped_tokens(self, tokens);
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

impl RecoverySyntaxSink for StructFieldDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        StructFieldDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
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

impl RecoverySyntaxSink for VariantDirectivesSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        VariantDirectivesSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for UnionVariantDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        UnionVariantDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for UnionVariantPayloadSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        UnionVariantPayloadSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for UnionPayloadFieldSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        UnionPayloadFieldSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TypeCallableMemberDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TypeCallableMemberDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TypeConstructorMemberDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TypeConstructorMemberDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for FinalizerMemberDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        FinalizerMemberDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for DestructorMemberDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        DestructorMemberDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ScopeEnterMemberDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ScopeEnterMemberDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ScopeExitMemberDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ScopeExitMemberDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitCallableMemberDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitCallableMemberDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitConstantMemberDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitConstantMemberDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitTypeMemberDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitTypeMemberDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitPredicateMemberDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitPredicateMemberDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitFinalizerRequirementDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitFinalizerRequirementDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitDestructorRequirementDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitDestructorRequirementDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitScopeEnterRequirementDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitScopeEnterRequirementDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitScopeExitRequirementDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitScopeExitRequirementDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
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

impl RecoverySyntaxSink for RequiresClauseSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        RequiresClauseSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for EnsuresClauseSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        EnsuresClauseSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for WithClauseSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        WithClauseSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for UsesClauseSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        UsesClauseSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for BlockExpressionSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        BlockExpressionSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for BlockItemSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        BlockItemSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for LocalBindingDeclarationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        LocalBindingDeclarationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for SequencedExpressionSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        SequencedExpressionSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TypeExpressionSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TypeExpressionSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ExpressionSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ExpressionSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for PrimaryExpressionSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        PrimaryExpressionSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for AccessExpressionSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        AccessExpressionSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for MemberAccessOperationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        MemberAccessOperationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ElementIndexOperationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ElementIndexOperationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ArgumentListSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ArgumentListSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ArgumentSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ArgumentSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for SliceIndexOperationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        SliceIndexOperationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ConversionOperationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ConversionOperationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TraitQualifiedMemberOperationSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TraitQualifiedMemberOperationSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for StructConstructionBodySyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        StructConstructionBodySyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for StructFieldInitializerSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        StructFieldInitializerSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for GroupedExpressionSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        GroupedExpressionSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for TupleExpressionSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        TupleExpressionSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl RecoverySyntaxSink for ArrayExpressionSyntaxBuilder {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        ArrayExpressionSyntaxBuilder::push_skipped_tokens(self, tokens);
    }
}

impl_recovery_syntax_sink!(
    GeneralGeneratorExpressionSyntaxBuilder,
    GeneratorIterationExpressionSyntaxBuilder,
    ConditionalExpressionSyntaxBuilder,
    ConditionalElseSyntaxBuilder,
    MatchExpressionSyntaxBuilder,
    MatchSubjectSyntaxBuilder,
    MatchBodySyntaxBuilder,
    MatchArmSyntaxBuilder,
    WhileExpressionSyntaxBuilder,
    ForExpressionSyntaxBuilder,
    IterationSourceSyntaxBuilder,
    LoopExpressionSyntaxBuilder,
    WithExpressionSyntaxBuilder,
    LambdaExpressionSyntaxBuilder,
    BorrowExpressionSyntaxBuilder,
    TrustBoundaryExpressionSyntaxBuilder,
    AssertionExpressionSyntaxBuilder,
    ResultPropagationExpressionSyntaxBuilder,
    CatchExpressionSyntaxBuilder,
    AwaitExpressionSyntaxBuilder,
    AsyncBlockExpressionSyntaxBuilder,
    SpawnExpressionSyntaxBuilder,
    TypeFormConstructionExpressionSyntaxBuilder,
    BooleanFoldExpressionSyntaxBuilder,
    YieldExpressionSyntaxBuilder,
    ReturnExpressionSyntaxBuilder,
    PanicExpressionSyntaxBuilder,
    BreakExpressionSyntaxBuilder,
    ContinueExpressionSyntaxBuilder,
);

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

impl BracedBodySyntaxSink for ImplementationBodySyntaxBuilder {
    fn push_open_brace_token(&mut self, token: SyntaxToken) {
        ImplementationBodySyntaxBuilder::push_open_brace_token(self, token);
    }

    fn push_close_brace_token(&mut self, token: SyntaxToken) {
        ImplementationBodySyntaxBuilder::push_close_brace_token(self, token);
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

impl BracedBodySyntaxSink for BlockExpressionSyntaxBuilder {
    fn push_open_brace_token(&mut self, token: SyntaxToken) {
        BlockExpressionSyntaxBuilder::push_open_brace_token(self, token);
    }

    fn push_close_brace_token(&mut self, token: SyntaxToken) {
        BlockExpressionSyntaxBuilder::push_close_brace_token(self, token);
    }
}

impl BracedBodySyntaxSink for MatchBodySyntaxBuilder {
    fn push_open_brace_token(&mut self, token: SyntaxToken) {
        MatchBodySyntaxBuilder::push_open_brace_token(self, token);
    }

    fn push_close_brace_token(&mut self, token: SyntaxToken) {
        MatchBodySyntaxBuilder::push_close_brace_token(self, token);
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

        assert_eq!(diagnostic_kinds(&diagnostics), []);
    }
}
