use bray_syntax::{
    CallableOverloadDeclarationSyntax, ConstantDeclarationSyntax,
    DestructorMemberDeclarationSyntax, FinalizerMemberDeclarationSyntax,
    ImplementationTypeMemberBindingSyntax, PredicateDeclarationSyntax,
    ScopeEnterMemberDeclarationSyntax, ScopeExitMemberDeclarationSyntax,
    StructFieldDeclarationSyntax, SyntaxKind, SyntaxToken, TraitCallableMemberDeclarationSyntax,
    TraitConstantMemberDeclarationSyntax, TraitDestructorRequirementDeclarationSyntax,
    TraitFinalizerRequirementDeclarationSyntax, TraitPredicateMemberDeclarationSyntax,
    TraitScopeEnterRequirementDeclarationSyntax, TraitScopeExitRequirementDeclarationSyntax,
    TraitTypeMemberDeclarationSyntax, TypeCallableMemberDeclarationSyntax,
    TypeConstructorMemberDeclarationSyntax, UnionVariantDeclarationSyntax,
};

use super::entry::{DeclarationFragmentContext, DeclarationFragmentSyntax};
use super::member::{
    ImplementationBodyItemSyntaxSink, SharedTypeMemberSyntaxSink, StructBodyItemSyntaxSink,
    TraitBodyItemSyntaxSink, TraitLifecycleRequirementSyntaxSink, TypeLifecycleMemberSyntaxSink,
    UnionBodyItemSyntaxSink,
};
use super::recovery::RecoverySyntaxSink;
use super::state::Parser;

impl Parser {
    pub(super) fn parse_declaration_fragment(
        &mut self,
        context: DeclarationFragmentContext,
    ) -> (Option<DeclarationFragmentSyntax>, bool) {
        let mut sink = DeclarationFragmentSink::new();

        match context {
            DeclarationFragmentContext::Module => self.parse_module_declaration_fragment(&mut sink),
            DeclarationFragmentContext::Struct => self.parse_struct_body_item(&mut sink),
            DeclarationFragmentContext::Union => self.parse_union_body_item(&mut sink),
            DeclarationFragmentContext::UnionVariant => {
                sink.push(DeclarationFragmentSyntax::UnionPayloadField(
                    self.parse_union_payload_field(),
                ));
            }
            DeclarationFragmentContext::Trait => self.parse_trait_body_item(&mut sink),
            DeclarationFragmentContext::Implementation => {
                self.parse_implementation_body_item(&mut sink);
            }
        }

        self.finish_fragment(&mut sink);

        sink.is_recovered |= sink
            .declaration
            .as_ref()
            .is_none_or(DeclarationFragmentSyntax::is_recovered);

        sink.finish()
    }

    pub(super) fn parse_type_expression_fragment(
        &mut self,
    ) -> (bray_syntax::TypeExpressionSyntax, bool) {
        let mut at_end = |parser: &mut Parser| parser.at(SyntaxKind::EndOfFileToken);
        let type_expression = self.parse_type_expression_until(&mut at_end);

        let mut sink = DeclarationFragmentSink::new();

        self.finish_fragment(&mut sink);

        let is_recovered = type_expression.is_recovered() || sink.is_recovered;

        (type_expression, is_recovered)
    }

    fn finish_fragment(&mut self, sink: &mut DeclarationFragmentSink) {
        self.recover_until(sink, &[SyntaxKind::EndOfFileToken]);
        self.expect(SyntaxKind::EndOfFileToken);
    }

    fn parse_module_declaration_fragment(&mut self, sink: &mut DeclarationFragmentSink) {
        if let Some(declaration) = self.parse_module_declaration() {
            sink.push(declaration);
        }
    }
}

struct DeclarationFragmentSink {
    declaration: Option<DeclarationFragmentSyntax>,
    is_recovered: bool,
}

impl DeclarationFragmentSink {
    const fn new() -> Self {
        Self {
            declaration: None,
            is_recovered: false,
        }
    }

    fn push(&mut self, declaration: DeclarationFragmentSyntax) {
        self.declaration = Some(declaration);
    }

    fn finish(self) -> (Option<DeclarationFragmentSyntax>, bool) {
        (self.declaration, self.is_recovered)
    }
}

impl RecoverySyntaxSink for DeclarationFragmentSink {
    fn push_skipped_tokens(&mut self, tokens: Vec<SyntaxToken>) {
        self.is_recovered |= !tokens.is_empty();
    }
}

impl SharedTypeMemberSyntaxSink for DeclarationFragmentSink {
    fn push_type_callable_member_declaration(
        &mut self,
        declaration: TypeCallableMemberDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::TypeCallableMember(declaration));
    }

    fn push_type_constructor_member_declaration(
        &mut self,
        declaration: TypeConstructorMemberDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::TypeConstructorMember(
            declaration,
        ));
    }

    fn push_constant_declaration(&mut self, declaration: ConstantDeclarationSyntax) {
        self.push(DeclarationFragmentSyntax::Constant(declaration));
    }

    fn push_predicate_declaration(&mut self, declaration: PredicateDeclarationSyntax) {
        self.push(DeclarationFragmentSyntax::Predicate(declaration));
    }

    fn push_callable_overload_declaration(
        &mut self,
        declaration: CallableOverloadDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::CallableOverload(declaration));
    }
}

impl StructBodyItemSyntaxSink for DeclarationFragmentSink {
    fn push_struct_field_declaration(&mut self, declaration: StructFieldDeclarationSyntax) {
        self.push(DeclarationFragmentSyntax::StructField(declaration));
    }
}

impl UnionBodyItemSyntaxSink for DeclarationFragmentSink {
    fn push_union_variant_declaration(&mut self, declaration: UnionVariantDeclarationSyntax) {
        self.push(DeclarationFragmentSyntax::UnionVariant(declaration));
    }
}

impl TraitBodyItemSyntaxSink for DeclarationFragmentSink {
    fn push_trait_callable_member_declaration(
        &mut self,
        declaration: TraitCallableMemberDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::TraitCallableMember(declaration));
    }

    fn push_trait_constant_member_declaration(
        &mut self,
        declaration: TraitConstantMemberDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::TraitConstantMember(declaration));
    }

    fn push_trait_type_member_declaration(
        &mut self,
        declaration: TraitTypeMemberDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::TraitTypeMember(declaration));
    }

    fn push_trait_predicate_member_declaration(
        &mut self,
        declaration: TraitPredicateMemberDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::TraitPredicateMember(declaration));
    }
}

impl ImplementationBodyItemSyntaxSink for DeclarationFragmentSink {
    fn push_implementation_type_member_binding(
        &mut self,
        binding: ImplementationTypeMemberBindingSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::ImplementationTypeMemberBinding(
            binding,
        ));
    }
}

impl TypeLifecycleMemberSyntaxSink for DeclarationFragmentSink {
    fn push_finalizer_member_declaration(&mut self, declaration: FinalizerMemberDeclarationSyntax) {
        self.push(DeclarationFragmentSyntax::FinalizerMember(declaration));
    }

    fn push_destructor_member_declaration(
        &mut self,
        declaration: DestructorMemberDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::DestructorMember(declaration));
    }

    fn push_scope_enter_member_declaration(
        &mut self,
        declaration: ScopeEnterMemberDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::ScopeEnterMember(declaration));
    }

    fn push_scope_exit_member_declaration(
        &mut self,
        declaration: ScopeExitMemberDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::ScopeExitMember(declaration));
    }
}

impl TraitLifecycleRequirementSyntaxSink for DeclarationFragmentSink {
    fn push_trait_finalizer_requirement_declaration(
        &mut self,
        declaration: TraitFinalizerRequirementDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::TraitFinalizerRequirement(
            declaration,
        ));
    }

    fn push_trait_destructor_requirement_declaration(
        &mut self,
        declaration: TraitDestructorRequirementDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::TraitDestructorRequirement(
            declaration,
        ));
    }

    fn push_trait_scope_enter_requirement_declaration(
        &mut self,
        declaration: TraitScopeEnterRequirementDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::TraitScopeEnterRequirement(
            declaration,
        ));
    }

    fn push_trait_scope_exit_requirement_declaration(
        &mut self,
        declaration: TraitScopeExitRequirementDeclarationSyntax,
    ) {
        self.push(DeclarationFragmentSyntax::TraitScopeExitRequirement(
            declaration,
        ));
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::SyntaxText;
    use bray_testing::test_source_store as source_store;

    use super::super::entry::{
        DeclarationFragmentContext, DeclarationFragmentSyntax, parse_declaration_fragment,
        parse_type_expression_fragment,
    };
    use crate::test_support::{diagnostic_kinds, source};

    #[test]
    fn declaration_fragment_entry_point_parses_one_module_declaration() {
        let sources = source_store(["trusted func copy<T>(value: T) -> T;"]);

        let result =
            parse_declaration_fragment(&source(&sources, 0), DeclarationFragmentContext::Module);

        let Some(DeclarationFragmentSyntax::Function(declaration)) = result.declaration() else {
            panic!("expected function declaration fragment");
        };

        assert_eq!(
            declaration.full_text(),
            "trusted func copy<T>(value: T) -> T;"
        );

        assert!(result.diagnostics().is_empty());
        assert!(!result.is_recovered());
    }

    #[test]
    fn declaration_fragment_context_selects_member_grammar() {
        let struct_sources = source_store(["value: Int;"]);
        let union_sources = source_store(["Some(value: Int);"]);
        let union_variant_sources = source_store(["value: Int"]);
        let trait_sources = source_store(["func read() -> Int;"]);
        let implementation_sources = source_store(["type Item = Int;"]);

        let struct_result = parse_declaration_fragment(
            &source(&struct_sources, 0),
            DeclarationFragmentContext::Struct,
        );
        let trait_result = parse_declaration_fragment(
            &source(&trait_sources, 0),
            DeclarationFragmentContext::Trait,
        );
        let union_result = parse_declaration_fragment(
            &source(&union_sources, 0),
            DeclarationFragmentContext::Union,
        );
        let union_variant_result = parse_declaration_fragment(
            &source(&union_variant_sources, 0),
            DeclarationFragmentContext::UnionVariant,
        );
        let implementation_result = parse_declaration_fragment(
            &source(&implementation_sources, 0),
            DeclarationFragmentContext::Implementation,
        );

        assert!(matches!(
            struct_result.declaration(),
            Some(DeclarationFragmentSyntax::StructField(_))
        ));

        assert!(matches!(
            trait_result.declaration(),
            Some(DeclarationFragmentSyntax::TraitCallableMember(_))
        ));

        assert!(matches!(
            union_result.declaration(),
            Some(DeclarationFragmentSyntax::UnionVariant(_))
        ));

        assert!(matches!(
            union_variant_result.declaration(),
            Some(DeclarationFragmentSyntax::UnionPayloadField(_))
        ));

        assert!(matches!(
            implementation_result.declaration(),
            Some(DeclarationFragmentSyntax::ImplementationTypeMemberBinding(
                _
            ))
        ));

        assert!(!struct_result.is_recovered());
        assert!(!trait_result.is_recovered());
        assert!(!union_result.is_recovered());
        assert!(!union_variant_result.is_recovered());
        assert!(!implementation_result.is_recovered());
    }

    #[test]
    fn declaration_fragment_entry_point_preserves_syntax_diagnostics_and_recovery() {
        let sources = source_store(["func copy(value Int);"]);

        let result =
            parse_declaration_fragment(&source(&sources, 0), DeclarationFragmentContext::Module);

        assert!(matches!(
            result.declaration(),
            Some(DeclarationFragmentSyntax::Function(_))
        ));

        assert_eq!(
            diagnostic_kinds(result.diagnostics()),
            [DiagnosticKind::SyntaxExpectedToken]
        );

        assert!(result.is_recovered());
    }

    #[test]
    fn declaration_fragment_entry_point_marks_wrong_context_and_extra_syntax_as_recovery() {
        let wrong_context_sources = source_store(["struct Item {}"]);
        let extra_sources = source_store(["struct First {} struct Second {}"]);

        let wrong_context = parse_declaration_fragment(
            &source(&wrong_context_sources, 0),
            DeclarationFragmentContext::Trait,
        );
        let extra = parse_declaration_fragment(
            &source(&extra_sources, 0),
            DeclarationFragmentContext::Module,
        );

        assert!(wrong_context.declaration().is_none());
        assert!(wrong_context.is_recovered());

        assert!(matches!(
            extra.declaration(),
            Some(DeclarationFragmentSyntax::Struct(_))
        ));

        assert!(extra.is_recovered());
    }

    #[test]
    fn type_expression_fragment_entry_point_reuses_type_parser_and_preserves_recovery() {
        let valid_sources = source_store(["func(value: T) -> box T"]);
        let invalid_sources = source_store(["$"]);
        let extra_sources = source_store(["Int Bool"]);

        let valid = parse_type_expression_fragment(&source(&valid_sources, 0));
        let invalid = parse_type_expression_fragment(&source(&invalid_sources, 0));
        let extra = parse_type_expression_fragment(&source(&extra_sources, 0));

        assert_eq!(
            valid.type_expression().full_text(),
            "func(value: T) -> box T"
        );

        assert!(valid.diagnostics().is_empty());
        assert!(!valid.is_recovered());

        assert_eq!(
            diagnostic_kinds(invalid.diagnostics()),
            [DiagnosticKind::LexicalInvalidCharacter]
        );

        assert!(invalid.is_recovered());

        assert!(extra.diagnostics().is_empty());
        assert!(extra.is_recovered());
    }
}
