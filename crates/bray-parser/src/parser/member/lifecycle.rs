use bray_syntax::{
    AsyncCapableLifecycleMemberModifiersSyntax, CallableBodyBlockExpressionSyntax,
    CallableResultClauseSyntax, DestructorMemberDeclarationSyntax,
    DestructorMemberDeclarationSyntaxBuilder, FinalizerMemberDeclarationSyntax,
    FinalizerMemberDeclarationSyntaxBuilder, ImplementationBodySyntaxBuilder, ParameterListSyntax,
    ScopeEnterMemberDeclarationSyntax, ScopeEnterMemberDeclarationSyntaxBuilder,
    ScopeExitMemberDeclarationSyntax, ScopeExitMemberDeclarationSyntaxBuilder,
    StructBodySyntaxBuilder, SyncLifecycleMemberModifiersSyntax, SyntaxKind, SyntaxToken,
    TraitBodySyntaxBuilder, TraitDestructorRequirementDeclarationSyntax,
    TraitDestructorRequirementDeclarationSyntaxBuilder, TraitFinalizerRequirementDeclarationSyntax,
    TraitFinalizerRequirementDeclarationSyntaxBuilder, TraitScopeEnterRequirementDeclarationSyntax,
    TraitScopeEnterRequirementDeclarationSyntaxBuilder, TraitScopeExitRequirementDeclarationSyntax,
    TraitScopeExitRequirementDeclarationSyntaxBuilder, UnionBodySyntaxBuilder,
};

use crate::parser::contract::CallableContractClauseSyntaxSink;
use crate::parser::member::body::{MEMBER_ITEM_RECOVERY_KINDS, MEMBER_KEYWORD_RECOVERY_KINDS};
use crate::parser::recovery::RecoverySyntaxSink;
use crate::parser::state::Parser;

const ASYNC_CAPABLE_LIFECYCLE_MEMBER_START_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::FinalizeKeyword,
];

const SYNC_LIFECYCLE_MEMBER_START_KINDS: [SyntaxKind; 2] =
    [SyntaxKind::TrustedKeyword, SyntaxKind::DestructKeyword];

const SCOPE_LIFECYCLE_MEMBER_START_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::AsyncKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::EnterKeyword,
    SyntaxKind::ExitKeyword,
];

const LIFECYCLE_CONTRACT_TAIL_BOUNDARY_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::OpenBraceToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const LIFECYCLE_BODY_MISSING_BOUNDARY_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const LIFECYCLE_REQUIREMENT_END_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleResultPolicy {
    Optional,
    Required,
}

impl Parser {
    pub(super) fn parse_type_lifecycle_member_declaration(
        &mut self,
        builder: &mut impl TypeLifecycleMemberSyntaxSink,
    ) {
        if self.should_parse_finalizer_member_declaration() {
            builder.push_finalizer_member_declaration(self.parse_finalizer_member_declaration());
            return;
        }

        if self.should_parse_destructor_member_declaration() {
            builder.push_destructor_member_declaration(self.parse_destructor_member_declaration());
            return;
        }

        if self.should_parse_scope_enter_member_declaration() {
            builder
                .push_scope_enter_member_declaration(self.parse_scope_enter_member_declaration());
            return;
        }

        if self.should_parse_scope_exit_member_declaration() {
            builder.push_scope_exit_member_declaration(self.parse_scope_exit_member_declaration());
            return;
        }

        self.recover_current_token(builder);
    }

    pub(super) fn parse_trait_lifecycle_requirement_declaration(
        &mut self,
        builder: &mut impl TraitLifecycleRequirementSyntaxSink,
    ) {
        if self.should_parse_trait_finalizer_requirement_declaration() {
            builder.push_trait_finalizer_requirement_declaration(
                self.parse_trait_finalizer_requirement_declaration(),
            );
            return;
        }

        if self.should_parse_trait_destructor_requirement_declaration() {
            builder.push_trait_destructor_requirement_declaration(
                self.parse_trait_destructor_requirement_declaration(),
            );
            return;
        }

        if self.should_parse_trait_scope_enter_requirement_declaration() {
            builder.push_trait_scope_enter_requirement_declaration(
                self.parse_trait_scope_enter_requirement_declaration(),
            );
            return;
        }

        if self.should_parse_trait_scope_exit_requirement_declaration() {
            builder.push_trait_scope_exit_requirement_declaration(
                self.parse_trait_scope_exit_requirement_declaration(),
            );
            return;
        }

        self.recover_current_token(builder);
    }

    fn parse_finalizer_member_declaration(&mut self) -> FinalizerMemberDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = FinalizerMemberDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_async_capable_lifecycle_member_modifiers(
            self.parse_async_capable_lifecycle_member_modifiers(),
        );

        builder.push_finalize_keyword(self.expect(SyntaxKind::FinalizeKeyword));
        self.parse_lifecycle_body_tail(&mut builder, LifecycleResultPolicy::Optional);

        builder.build()
    }

    fn parse_destructor_member_declaration(&mut self) -> DestructorMemberDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = DestructorMemberDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_sync_lifecycle_member_modifiers(self.parse_sync_lifecycle_member_modifiers());
        builder.push_destruct_keyword(self.expect(SyntaxKind::DestructKeyword));
        self.parse_lifecycle_body_tail(&mut builder, LifecycleResultPolicy::Optional);

        builder.build()
    }

    fn parse_scope_enter_member_declaration(&mut self) -> ScopeEnterMemberDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ScopeEnterMemberDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_async_capable_lifecycle_member_modifiers(
            self.parse_async_capable_lifecycle_member_modifiers(),
        );

        builder.push_enter_keyword(self.expect(SyntaxKind::EnterKeyword));
        self.parse_lifecycle_body_tail(&mut builder, LifecycleResultPolicy::Required);

        builder.build()
    }

    fn parse_scope_exit_member_declaration(&mut self) -> ScopeExitMemberDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ScopeExitMemberDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_async_capable_lifecycle_member_modifiers(
            self.parse_async_capable_lifecycle_member_modifiers(),
        );

        builder.push_exit_keyword(self.expect(SyntaxKind::ExitKeyword));
        self.parse_lifecycle_body_tail(&mut builder, LifecycleResultPolicy::Optional);

        builder.build()
    }

    fn parse_trait_finalizer_requirement_declaration(
        &mut self,
    ) -> TraitFinalizerRequirementDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            TraitFinalizerRequirementDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_async_capable_lifecycle_member_modifiers(
            self.parse_async_capable_lifecycle_member_modifiers(),
        );

        builder.push_finalize_keyword(self.expect(SyntaxKind::FinalizeKeyword));
        self.parse_lifecycle_requirement_tail(&mut builder, LifecycleResultPolicy::Optional);

        builder.build()
    }

    fn parse_trait_destructor_requirement_declaration(
        &mut self,
    ) -> TraitDestructorRequirementDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            TraitDestructorRequirementDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_sync_lifecycle_member_modifiers(self.parse_sync_lifecycle_member_modifiers());
        builder.push_destruct_keyword(self.expect(SyntaxKind::DestructKeyword));
        self.parse_lifecycle_requirement_tail(&mut builder, LifecycleResultPolicy::Optional);

        builder.build()
    }

    fn parse_trait_scope_enter_requirement_declaration(
        &mut self,
    ) -> TraitScopeEnterRequirementDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            TraitScopeEnterRequirementDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_async_capable_lifecycle_member_modifiers(
            self.parse_async_capable_lifecycle_member_modifiers(),
        );

        builder.push_enter_keyword(self.expect(SyntaxKind::EnterKeyword));
        self.parse_lifecycle_requirement_tail(&mut builder, LifecycleResultPolicy::Required);

        builder.build()
    }

    fn parse_trait_scope_exit_requirement_declaration(
        &mut self,
    ) -> TraitScopeExitRequirementDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            TraitScopeExitRequirementDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_async_capable_lifecycle_member_modifiers(
            self.parse_async_capable_lifecycle_member_modifiers(),
        );

        builder.push_exit_keyword(self.expect(SyntaxKind::ExitKeyword));
        self.parse_lifecycle_requirement_tail(&mut builder, LifecycleResultPolicy::Optional);

        builder.build()
    }

    fn parse_async_capable_lifecycle_member_modifiers(
        &mut self,
    ) -> AsyncCapableLifecycleMemberModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder =
            AsyncCapableLifecycleMemberModifiersSyntax::builder(self.syntax_source(), start);

        while self.at_async_capable_lifecycle_member_modifier() {
            if self.at(SyntaxKind::AsyncKeyword) {
                builder.push_async_token(self.expect(SyntaxKind::AsyncKeyword));
                continue;
            }

            builder.push_trusted_token(self.expect(SyntaxKind::TrustedKeyword));
        }

        builder.build()
    }

    fn parse_sync_lifecycle_member_modifiers(&mut self) -> SyncLifecycleMemberModifiersSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SyncLifecycleMemberModifiersSyntax::builder(self.syntax_source(), start);

        while self.at_sync_lifecycle_member_modifier() {
            builder.push_trusted_token(self.expect(SyntaxKind::TrustedKeyword));
        }

        builder.build()
    }

    fn parse_lifecycle_body_tail(
        &mut self,
        builder: &mut impl LifecycleBodySyntaxSink,
        result_policy: LifecycleResultPolicy,
    ) {
        self.parse_lifecycle_signature(builder, result_policy);

        builder.push_callable_body_block_expression(
            self.parse_callable_body_block_expression_until(
                Parser::at_lifecycle_body_missing_boundary,
            ),
        );
    }

    fn parse_lifecycle_requirement_tail(
        &mut self,
        builder: &mut impl LifecycleRequirementSyntaxSink,
        result_policy: LifecycleResultPolicy,
    ) {
        self.parse_lifecycle_signature(builder, result_policy);
        self.recover_until_predicate(builder, Parser::at_lifecycle_requirement_end);

        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));
    }

    fn parse_lifecycle_signature(
        &mut self,
        builder: &mut impl LifecycleSignatureSyntaxSink,
        result_policy: LifecycleResultPolicy,
    ) {
        builder.push_parameter_list(self.parse_parameter_list());

        match result_policy {
            LifecycleResultPolicy::Required => {
                builder.push_callable_result_clause(
                    self.parse_callable_result_clause_until(
                        Parser::at_lifecycle_result_type_boundary,
                    ),
                );
            }
            LifecycleResultPolicy::Optional => {
                if self.at(SyntaxKind::ArrowToken) {
                    builder.push_callable_result_clause(self.parse_callable_result_clause_until(
                        Parser::at_lifecycle_result_type_boundary,
                    ));
                }
            }
        }

        self.parse_callable_contract_clauses(builder, Parser::at_lifecycle_contract_boundary);
    }

    fn at_lifecycle_result_type_boundary(&mut self) -> bool {
        self.at_lifecycle_contract_boundary()
    }

    fn at_lifecycle_contract_boundary(&mut self) -> bool {
        self.at_callable_contract_clause_start()
            || self.at_any(&LIFECYCLE_CONTRACT_TAIL_BOUNDARY_KINDS)
            || self.at_any(&MEMBER_KEYWORD_RECOVERY_KINDS)
    }

    fn at_lifecycle_body_missing_boundary(&mut self) -> bool {
        self.at_any(&LIFECYCLE_BODY_MISSING_BOUNDARY_KINDS)
            || self.at_any(&MEMBER_ITEM_RECOVERY_KINDS)
    }

    fn at_lifecycle_requirement_end(&mut self) -> bool {
        self.at_any(&LIFECYCLE_REQUIREMENT_END_KINDS) || self.at_any(&MEMBER_KEYWORD_RECOVERY_KINDS)
    }

    pub(super) fn should_parse_type_lifecycle_member_declaration(&mut self) -> bool {
        self.should_parse_finalizer_member_declaration()
            || self.should_parse_destructor_member_declaration()
            || self.should_parse_scope_enter_member_declaration()
            || self.should_parse_scope_exit_member_declaration()
    }

    pub(super) fn should_parse_trait_lifecycle_requirement_declaration(&mut self) -> bool {
        self.should_parse_trait_finalizer_requirement_declaration()
            || self.should_parse_trait_destructor_requirement_declaration()
            || self.should_parse_trait_scope_enter_requirement_declaration()
            || self.should_parse_trait_scope_exit_requirement_declaration()
    }

    fn should_parse_finalizer_member_declaration(&mut self) -> bool {
        self.at_async_capable_lifecycle_start(SyntaxKind::FinalizeKeyword)
    }

    fn should_parse_destructor_member_declaration(&mut self) -> bool {
        self.at_sync_lifecycle_start(SyntaxKind::DestructKeyword)
    }

    fn should_parse_scope_enter_member_declaration(&mut self) -> bool {
        self.at_scope_lifecycle_start(SyntaxKind::EnterKeyword)
    }

    fn should_parse_scope_exit_member_declaration(&mut self) -> bool {
        self.at_scope_lifecycle_start(SyntaxKind::ExitKeyword)
    }

    fn should_parse_trait_finalizer_requirement_declaration(&mut self) -> bool {
        self.at_async_capable_lifecycle_start(SyntaxKind::FinalizeKeyword)
    }

    fn should_parse_trait_destructor_requirement_declaration(&mut self) -> bool {
        self.at_sync_lifecycle_start(SyntaxKind::DestructKeyword)
    }

    fn should_parse_trait_scope_enter_requirement_declaration(&mut self) -> bool {
        self.at_scope_lifecycle_start(SyntaxKind::EnterKeyword)
    }

    fn should_parse_trait_scope_exit_requirement_declaration(&mut self) -> bool {
        self.at_scope_lifecycle_start(SyntaxKind::ExitKeyword)
    }

    fn at_async_capable_lifecycle_start(&mut self, keyword: SyntaxKind) -> bool {
        if !self.at(keyword) && !self.at_any(&ASYNC_CAPABLE_LIFECYCLE_MEMBER_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_async_capable_lifecycle_member_modifiers_for_scan();

            scan.at(keyword)
        })
    }

    fn at_sync_lifecycle_start(&mut self, keyword: SyntaxKind) -> bool {
        if !self.at(keyword) && !self.at_any(&SYNC_LIFECYCLE_MEMBER_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_sync_lifecycle_member_modifiers_for_scan();

            scan.at(keyword)
        })
    }

    fn at_scope_lifecycle_start(&mut self, keyword: SyntaxKind) -> bool {
        if !self.at(keyword) && !self.at_any(&SCOPE_LIFECYCLE_MEMBER_START_KINDS) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.consume_async_capable_lifecycle_member_modifiers_for_scan();

            scan.at(keyword)
        })
    }

    fn at_async_capable_lifecycle_member_modifier(&mut self) -> bool {
        self.at(SyntaxKind::AsyncKeyword) || self.at(SyntaxKind::TrustedKeyword)
    }

    fn at_sync_lifecycle_member_modifier(&mut self) -> bool {
        self.at(SyntaxKind::TrustedKeyword)
    }

    fn consume_async_capable_lifecycle_member_modifiers_for_scan(&mut self) {
        while self.at_async_capable_lifecycle_member_modifier() {
            self.consume();
        }
    }

    fn consume_sync_lifecycle_member_modifiers_for_scan(&mut self) {
        while self.at_sync_lifecycle_member_modifier() {
            self.consume();
        }
    }
}

pub(super) trait TypeLifecycleMemberSyntaxSink: RecoverySyntaxSink {
    fn push_finalizer_member_declaration(&mut self, declaration: FinalizerMemberDeclarationSyntax);

    fn push_destructor_member_declaration(
        &mut self,
        declaration: DestructorMemberDeclarationSyntax,
    );

    fn push_scope_enter_member_declaration(
        &mut self,
        declaration: ScopeEnterMemberDeclarationSyntax,
    );

    fn push_scope_exit_member_declaration(&mut self, declaration: ScopeExitMemberDeclarationSyntax);
}

pub(super) trait TraitLifecycleRequirementSyntaxSink: RecoverySyntaxSink {
    fn push_trait_finalizer_requirement_declaration(
        &mut self,
        declaration: TraitFinalizerRequirementDeclarationSyntax,
    );

    fn push_trait_destructor_requirement_declaration(
        &mut self,
        declaration: TraitDestructorRequirementDeclarationSyntax,
    );

    fn push_trait_scope_enter_requirement_declaration(
        &mut self,
        declaration: TraitScopeEnterRequirementDeclarationSyntax,
    );

    fn push_trait_scope_exit_requirement_declaration(
        &mut self,
        declaration: TraitScopeExitRequirementDeclarationSyntax,
    );
}

trait LifecycleSignatureSyntaxSink: RecoverySyntaxSink + CallableContractClauseSyntaxSink {
    fn push_parameter_list(&mut self, parameter_list: ParameterListSyntax);

    fn push_callable_result_clause(&mut self, clause: CallableResultClauseSyntax);
}

trait LifecycleBodySyntaxSink: LifecycleSignatureSyntaxSink {
    fn push_callable_body_block_expression(&mut self, body: CallableBodyBlockExpressionSyntax);
}

trait LifecycleRequirementSyntaxSink: LifecycleSignatureSyntaxSink {
    fn push_semicolon_token(&mut self, token: SyntaxToken);
}

macro_rules! impl_type_lifecycle_member_sink {
    ($builder:ty) => {
        impl TypeLifecycleMemberSyntaxSink for $builder {
            fn push_finalizer_member_declaration(
                &mut self,
                declaration: FinalizerMemberDeclarationSyntax,
            ) {
                <$builder>::push_finalizer_member_declaration(self, declaration);
            }

            fn push_destructor_member_declaration(
                &mut self,
                declaration: DestructorMemberDeclarationSyntax,
            ) {
                <$builder>::push_destructor_member_declaration(self, declaration);
            }

            fn push_scope_enter_member_declaration(
                &mut self,
                declaration: ScopeEnterMemberDeclarationSyntax,
            ) {
                <$builder>::push_scope_enter_member_declaration(self, declaration);
            }

            fn push_scope_exit_member_declaration(
                &mut self,
                declaration: ScopeExitMemberDeclarationSyntax,
            ) {
                <$builder>::push_scope_exit_member_declaration(self, declaration);
            }
        }
    };
}

macro_rules! impl_lifecycle_signature_sink {
    ($builder:ty) => {
        impl LifecycleSignatureSyntaxSink for $builder {
            fn push_parameter_list(&mut self, parameter_list: ParameterListSyntax) {
                <$builder>::push_parameter_list(self, parameter_list);
            }

            fn push_callable_result_clause(&mut self, clause: CallableResultClauseSyntax) {
                <$builder>::push_callable_result_clause(self, clause);
            }
        }
    };
}

macro_rules! impl_lifecycle_body_sink {
    ($builder:ty) => {
        impl_lifecycle_signature_sink!($builder);

        impl LifecycleBodySyntaxSink for $builder {
            fn push_callable_body_block_expression(
                &mut self,
                body: CallableBodyBlockExpressionSyntax,
            ) {
                <$builder>::push_callable_body_block_expression(self, body);
            }
        }
    };
}

macro_rules! impl_lifecycle_requirement_sink {
    ($builder:ty) => {
        impl_lifecycle_signature_sink!($builder);

        impl LifecycleRequirementSyntaxSink for $builder {
            fn push_semicolon_token(&mut self, token: SyntaxToken) {
                <$builder>::push_semicolon_token(self, token);
            }
        }
    };
}

impl_type_lifecycle_member_sink!(StructBodySyntaxBuilder);
impl_type_lifecycle_member_sink!(UnionBodySyntaxBuilder);
impl_type_lifecycle_member_sink!(ImplementationBodySyntaxBuilder);

impl TraitLifecycleRequirementSyntaxSink for TraitBodySyntaxBuilder {
    fn push_trait_finalizer_requirement_declaration(
        &mut self,
        declaration: TraitFinalizerRequirementDeclarationSyntax,
    ) {
        TraitBodySyntaxBuilder::push_trait_finalizer_requirement_declaration(self, declaration);
    }

    fn push_trait_destructor_requirement_declaration(
        &mut self,
        declaration: TraitDestructorRequirementDeclarationSyntax,
    ) {
        TraitBodySyntaxBuilder::push_trait_destructor_requirement_declaration(self, declaration);
    }

    fn push_trait_scope_enter_requirement_declaration(
        &mut self,
        declaration: TraitScopeEnterRequirementDeclarationSyntax,
    ) {
        TraitBodySyntaxBuilder::push_trait_scope_enter_requirement_declaration(self, declaration);
    }

    fn push_trait_scope_exit_requirement_declaration(
        &mut self,
        declaration: TraitScopeExitRequirementDeclarationSyntax,
    ) {
        TraitBodySyntaxBuilder::push_trait_scope_exit_requirement_declaration(self, declaration);
    }
}

impl_lifecycle_body_sink!(FinalizerMemberDeclarationSyntaxBuilder);
impl_lifecycle_body_sink!(DestructorMemberDeclarationSyntaxBuilder);
impl_lifecycle_body_sink!(ScopeEnterMemberDeclarationSyntaxBuilder);
impl_lifecycle_body_sink!(ScopeExitMemberDeclarationSyntaxBuilder);

impl_lifecycle_requirement_sink!(TraitFinalizerRequirementDeclarationSyntaxBuilder);
impl_lifecycle_requirement_sink!(TraitDestructorRequirementDeclarationSyntaxBuilder);
impl_lifecycle_requirement_sink!(TraitScopeEnterRequirementDeclarationSyntaxBuilder);
impl_lifecycle_requirement_sink!(TraitScopeExitRequirementDeclarationSyntaxBuilder);

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::TextRange;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::parser::parse_compilation_unit;
    use crate::test_support::{assert_missing_semicolon_diagnostic, marker_offset};

    // TODO(parser): Update this when callable body expressions are parsed.
    #[test]
    fn parser_parses_type_lifecycle_members() {
        let source = concat!(
            "module main; ",
            "struct Resource { ",
            "async finalize() -> Unit {} ",
            "trusted destruct() {} ",
            "enter() -> Guard {} ",
            "exit(scope: Guard) {} ",
            "}"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.struct_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one struct declaration: {declarations:?}");
        };

        let body = declaration.struct_body();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(body.finalizer_member_declarations().count(), 1);
        assert_eq!(body.destructor_member_declarations().count(), 1);
        assert_eq!(body.scope_enter_member_declarations().count(), 1);
        assert_eq!(body.scope_exit_member_declarations().count(), 1);

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_parses_trait_lifecycle_requirements() {
        let source = concat!(
            "module main; ",
            "trait Scope { async enter() -> Guard; exit(scope: Guard); }"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let body = declaration.trait_body();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(body.trait_scope_enter_requirement_declarations().count(), 1);
        assert_eq!(body.trait_scope_exit_requirement_declarations().count(), 1);

        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn parser_reports_missing_trait_lifecycle_requirement_semicolon() {
        let source = "module main; trait Scope { enter() -> Guard\nconst Id: Int; }";
        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit.trait_declarations().collect::<Vec<_>>();
        let insertion = marker_offset(source, "const");

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait declaration: {declarations:?}");
        };

        let requirements = declaration
            .trait_body()
            .trait_scope_enter_requirement_declarations()
            .collect::<Vec<_>>();

        let [requirement] = requirements.as_slice() else {
            panic!("expected one trait scope-enter requirement: {requirements:?}");
        };

        let semicolon_token = requirement.semicolon_token();

        assert_eq!(source_unit.full_text(), source);
        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_missing_semicolon_diagnostic(
            &result,
            insertion,
            SyntaxKind::ConstKeyword,
            "const",
            &[DiagnosticKind::SyntaxExpectedToken],
        );
    }

    // TODO(parser): Update this when callable body expressions are parsed.
    #[test]
    fn parser_parses_trait_implementation_lifecycle_members() {
        let source = concat!(
            "module main; ",
            "impl Resource(Scope) { enter() -> Guard {} exit(scope: Guard) {} }"
        );

        let sources = source_store([source]);
        let result = parse_compilation_unit(&sources);

        let source_unit = &result.syntax_tree().root().source_units()[0];
        let declarations = source_unit
            .unnamed_trait_implementation_declarations()
            .collect::<Vec<_>>();

        let [declaration] = declarations.as_slice() else {
            panic!("expected one trait implementation declaration: {declarations:?}");
        };

        let body = declaration.implementation_body();

        assert_eq!(source_unit.full_text(), source);
        assert_eq!(body.scope_enter_member_declarations().count(), 1);
        assert_eq!(body.scope_exit_member_declarations().count(), 1);

        assert!(result.diagnostics().is_empty());
    }
}
