use crate::SemanticUnitContext;
use bray_base::Cancellation;
use bray_bound_tree::BoundSourceAnchor;
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::DiagnosticResult;
use bray_source::{SourceSpan, TextRange, TextSize};
use bray_symbols::{
    AnySymbolId, AvailableCompilerKnownSymbols, DeclaredTypeRepresentation,
    GenericConstraintObligationKey, ImplementationRequirementKey, ImplementationSelection,
    MemberLookupResult, NamedTypeSymbolId, ProofOutcome, SemanticValueStore, StructSymbol,
    StructSymbolId, SymbolGraph, SymbolKey, SymbolName, SymbolQueryContract, SymbolQueryRequest,
    TraitApplicationId, TraitTypeMemberSymbolId, TypeId, UnionPayloadFieldSymbol,
    UnionPayloadFieldSymbolId, UnionSymbol, UnionSymbolId, UnionVariantSymbol,
    UnionVariantSymbolId,
};
use bray_target::TargetProfile;
use std::sync::Arc;

use super::errors::CheckerQueryResult;

/// The already-established generic premises available to one lexical checking unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericConstraintScope {
    /// The unit is a prerequisite of its owner's type header.
    Unavailable,
    /// Type and trait premises are established, but source predicates are being checked.
    TypeHeader,
    /// The owner's complete checked constraint set is available.
    Complete,
}

/// One recognized implementation hook and its availability for the selected target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ImplementationHookResolution {
    hook: ImplementationHook,
    available: bool,
}

impl ImplementationHookResolution {
    /// Creates a resolved implementation hook.
    pub const fn new(hook: ImplementationHook, available: bool) -> Self {
        Self { hook, available }
    }

    /// Returns the compiler implementation hook.
    pub const fn hook(self) -> ImplementationHook {
        self.hook
    }

    /// Returns whether the declaration is available for the selected target.
    pub const fn is_available(self) -> bool {
        self.available
    }
}

/// The exact source span and text covered by one bound source anchor.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CheckerSource<'source> {
    span: SourceSpan,
    text: &'source str,
}

impl<'source> CheckerSource<'source> {
    /// Creates a resolved checker source view.
    pub const fn new(span: SourceSpan, text: &'source str) -> Self {
        Self { span, text }
    }

    /// Returns the exact anchored source span.
    pub const fn span(self) -> SourceSpan {
        self.span
    }

    /// Returns the source text covered by the anchor.
    pub const fn text(self) -> &'source str {
        self.text
    }

    /// Returns text for an exact subrange of this resolved source view.
    pub fn text_for_range(self, range: TextRange) -> Option<&'source str> {
        if !self.span.range().contains_range(range) {
            return None;
        }

        let base = self.span.range().start().bytes();

        let relative = TextRange::new(
            TextSize::new(range.start().bytes() - base),
            TextSize::new(range.end().bytes() - base),
        );

        relative.slice_str(self.text)
    }
}

/// Narrow immutable services shared by checker unit views.
pub trait CheckerRequestContext: Sync {
    /// Exact failure type owned by the coordinating query layer.
    type UpstreamError;

    /// Returns the canonical semantic values used by bound structure and queries.
    fn semantic_values(&self) -> &SemanticValueStore;

    /// Returns checked type equalities available in this lexical checking context.
    fn type_equalities(&self) -> DiagnosticResult<&[(TypeId, TypeId)]> {
        DiagnosticResult::new(&[], bray_diagnostics::DiagnosticBag::new())
    }

    /// Identifies which owner premises precede the current checking unit.
    fn generic_constraint_scope(
        &self,
        context: &SemanticUnitContext,
        owner: bray_symbols::GenericOwnerId,
    ) -> CheckerQueryResult<DiagnosticResult<GenericConstraintScope>, Self::UpstreamError>;

    /// Matches inherited named-type arguments to an inherent callable's implementation header.
    fn inherited_callable_substitution(
        &self,
        instance: bray_symbols::CallableInstanceData,
    ) -> CheckerQueryResult<
        DiagnosticResult<Option<bray_symbols::GenericSubstitutionId>>,
        Self::UpstreamError,
    >;

    /// Resolves the open subject type that defines an implementation's contextual Self.
    fn implementation_subject_type(
        &self,
        implementation: bray_symbols::ImplementationSymbolId,
    ) -> CheckerQueryResult<DiagnosticResult<TypeId>, Self::UpstreamError>;

    /// Returns predicate contracts checked independently of callable body effects.
    fn callable_predicate_contracts(
        &self,
        _callable: bray_symbols::CallableSymbolId,
    ) -> CheckerQueryResult<
        bray_diagnostics::DiagnosticResult<std::sync::Arc<[bray_symbols::CallableContractClause]>>,
        Self::UpstreamError,
    > {
        Ok(bray_diagnostics::DiagnosticResult::new(
            std::sync::Arc::new([]),
            bray_diagnostics::DiagnosticBag::new(),
        ))
    }

    /// Returns the inferred contract retained by one callable's returned value.
    fn callable_result_dependencies(
        &self,
        callable: bray_symbols::CallableSymbolId,
    ) -> CheckerQueryResult<bray_symbols::DependencyContractTemplateId, Self::UpstreamError>;

    /// Returns the checked result contract of a statically selected anonymous body.
    fn anonymous_result_dependencies(
        &self,
        key: &bray_bound_tree::BoundUnitKey,
    ) -> CheckerQueryResult<bray_symbols::DependencyContractTemplateId, Self::UpstreamError>;

    /// Returns the checked result contract of a static initializer, independently of shutdown.
    fn static_result_dependencies(
        &self,
        declaration: bray_symbols::StaticSymbolId,
    ) -> CheckerQueryResult<bray_symbols::DependencyContractTemplateId, Self::UpstreamError>;

    /// Returns the exact trait requirement represented by an abstract dispatch route.
    fn dispatch_requirement(
        &self,
        dispatch: bray_symbols::TraitConstraintDispatch,
    ) -> CheckerQueryResult<ImplementationRequirementKey, Self::UpstreamError>;

    /// Resolves an abstract method when its implementation witness is available.
    fn witness_callable(
        &self,
        callable: bray_symbols::CallableInstanceId,
        requirement: ImplementationRequirementKey,
    ) -> CheckerQueryResult<
        Option<(
            bray_symbols::CallableInstanceData,
            bray_symbols::SelfTypeContext,
            bray_symbols::ImplementationInstanceId,
        )>,
        Self::UpstreamError,
    >;

    /// Returns the retained dependencies of a selected runtime-default provider.
    fn default_result_dependencies(
        &self,
        provider: bray_symbols::DefaultValueProvider,
    ) -> CheckerQueryResult<bray_symbols::DependencyContractTemplateId, Self::UpstreamError>;

    /// Returns the compilation-wide symbol graph.
    fn symbols(&self) -> &SymbolGraph;

    /// Returns the stable semantic key of a source or imported declaration.
    fn symbol_key(
        &self,
        symbol: AnySymbolId,
    ) -> CheckerQueryResult<Option<&SymbolKey>, Self::UpstreamError> {
        Ok(self.symbols().symbol_key(symbol))
    }

    /// Resolves one ordinary member from a source or imported declaration.
    fn lookup_member(
        &self,
        owner: AnySymbolId,
        name: &str,
    ) -> CheckerQueryResult<MemberLookupResult<AnySymbolId>, Self::UpstreamError> {
        Ok(self.symbols().lookup_member(owner, name))
    }

    /// Checks a member reference in the bound unit's lexical scope.
    fn check_member_access(
        &self,
        unit: bray_bound_tree::BoundUnitView<'_>,
        reference: bray_bound_tree::AnyBoundNodeId,
        owner: AnySymbolId,
        member: AnySymbolId,
    ) -> CheckerQueryResult<DiagnosticResult<bool>, Self::UpstreamError>;

    /// Returns the ordinary name of a source or imported declaration member.
    fn member_name(
        &self,
        member: AnySymbolId,
    ) -> CheckerQueryResult<Option<&SymbolName>, Self::UpstreamError> {
        Ok(self.symbols().member_name(member))
    }

    /// Returns a source or imported structure declaration.
    fn structure(
        &self,
        id: StructSymbolId,
    ) -> CheckerQueryResult<Option<&StructSymbol>, Self::UpstreamError> {
        Ok(self.symbols().structure(id))
    }

    /// Returns a source or imported structure field declaration.
    fn struct_field(
        &self,
        id: bray_symbols::StructFieldSymbolId,
    ) -> CheckerQueryResult<Option<&bray_symbols::StructFieldSymbol>, Self::UpstreamError> {
        Ok(self.symbols().struct_field(id))
    }

    /// Returns a source or imported union declaration.
    fn union(
        &self,
        id: UnionSymbolId,
    ) -> CheckerQueryResult<Option<&UnionSymbol>, Self::UpstreamError> {
        Ok(self.symbols().union(id))
    }

    /// Returns a source or imported union variant declaration.
    fn union_variant(
        &self,
        id: UnionVariantSymbolId,
    ) -> CheckerQueryResult<Option<&UnionVariantSymbol>, Self::UpstreamError> {
        Ok(self.symbols().union_variant(id))
    }

    /// Returns a source or imported union payload field declaration.
    fn union_payload_field(
        &self,
        id: UnionPayloadFieldSymbolId,
    ) -> CheckerQueryResult<Option<&UnionPayloadFieldSymbol>, Self::UpstreamError> {
        Ok(self.symbols().union_payload_field(id))
    }

    /// Returns compiler-known symbols available for the current target.
    fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols;

    /// Resolves a compiler-known or recognized standard-library implementation hook.
    fn implementation_hook(
        &self,
        symbol: AnySymbolId,
    ) -> CheckerQueryResult<Option<ImplementationHookResolution>, Self::UpstreamError> {
        let available = self.available_compiler_known_symbols();

        if let Some(hook) = available
            .provider()
            .role_registry()
            .symbol_implementation(symbol)
        {
            return Ok(Some(ImplementationHookResolution::new(
                hook,
                available.symbol_implementation(symbol).is_some(),
            )));
        }

        self.recognized_standard_library_implementation_hook(symbol)
    }

    /// Resolves an implementation hook carried by an imported standard-library declaration.
    fn recognized_standard_library_implementation_hook(
        &self,
        _symbol: AnySymbolId,
    ) -> CheckerQueryResult<Option<ImplementationHookResolution>, Self::UpstreamError> {
        Ok(None)
    }

    /// Returns the selected language-level target profile.
    fn selected_target(&self) -> &TargetProfile;

    /// Checks one source constant expression embedded in a type template.
    fn checked_constant_expression(
        &self,
        occurrence: bray_symbols::ConstantExpressionOccurrence,
    ) -> CheckerQueryResult<DiagnosticResult<bray_symbols::ConstantTermId>, Self::UpstreamError>;

    /// Normalizes a closed array extent while retaining unresolved generic terms.
    fn normalize_array_length(
        &self,
        term: bray_symbols::ConstantTermId,
    ) -> CheckerQueryResult<DiagnosticResult<bray_symbols::ConstantTermId>, Self::UpstreamError>;

    /// Proves the static constraints for one exact generic declaration instance.
    fn generic_constraints(
        &self,
        obligation: GenericConstraintObligationKey,
    ) -> CheckerQueryResult<DiagnosticResult<ProofOutcome>, Self::UpstreamError>;

    /// Selects the implementation satisfying one exact subject and trait application.
    fn implementation_selection(
        &self,
        _requirement: ImplementationRequirementKey,
    ) -> CheckerQueryResult<DiagnosticResult<ImplementationSelection>, Self::UpstreamError> {
        Ok(DiagnosticResult::without_diagnostics(
            ImplementationSelection::Unavailable,
        ))
    }

    /// Selects an implementation using exact trait constraints established by the active source context.
    fn implementation_selection_with_constraint_evidence(
        &self,
        requirement: ImplementationRequirementKey,
        _evidence: &[(TypeId, TraitApplicationId)],
    ) -> CheckerQueryResult<DiagnosticResult<ImplementationSelection>, Self::UpstreamError> {
        self.implementation_selection(requirement)
    }

    /// Resolves one selected type-valued member projection when its witness is available.
    fn selected_type_valued_member(
        &self,
        _subject: TypeId,
        _application: TraitApplicationId,
        _member: TraitTypeMemberSymbolId,
    ) -> CheckerQueryResult<DiagnosticResult<Option<TypeId>>, Self::UpstreamError> {
        Ok(DiagnosticResult::without_diagnostics(None))
    }

    /// Returns the checked representation contract for one declared type.
    fn declared_type_representation(
        &self,
        subject: NamedTypeSymbolId,
    ) -> CheckerQueryResult<DiagnosticResult<DeclaredTypeRepresentation>, Self::UpstreamError>;

    /// Returns the element of an authoritatively recognized raw-buffer declaration.
    fn raw_buffer_element(
        &self,
        definition: NamedTypeSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> CheckerQueryResult<Option<TypeId>, Self::UpstreamError>;

    /// Returns the target atomic representation selected for one concrete plain-storage type.
    fn plain_storage_atomic_representation(
        &self,
        _ty: TypeId,
    ) -> CheckerQueryResult<Option<bray_target::TargetAtomicRepresentation>, Self::UpstreamError>
    {
        Ok(None)
    }

    /// Selects one exact storage-policy protocol callable and its substituted signature.
    fn storage_protocol_callable(
        &self,
        _storage: TypeId,
        _target: TypeId,
        _member: &bray_compiler_known::CompilerKnownDeclarationKey,
    ) -> CheckerQueryResult<
        DiagnosticResult<
            Option<(
                bray_symbols::CallableInstanceData,
                bray_symbols::CallableSignature,
            )>,
        >,
        Self::UpstreamError,
    > {
        Ok(DiagnosticResult::without_diagnostics(None))
    }

    /// Returns the selected whole-value lifecycle implementation and substituted signature.
    fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: bray_symbols::TypeAssociatedLifecycleSlot,
    ) -> CheckerQueryResult<
        DiagnosticResult<
            Option<(
                bray_symbols::CallableInstanceData,
                bray_symbols::CallableSignature,
            )>,
        >,
        Self::UpstreamError,
    >;

    /// Returns the declared whole-value cleanup category before represented-part expansion.
    fn declared_type_lifecycle(
        &self,
        subject: NamedTypeSymbolId,
    ) -> CheckerQueryResult<DiagnosticResult<bray_symbols::TypeLifecycle>, Self::UpstreamError>;

    /// Returns whether the enclosing static context establishes a copy contract for an open type.
    fn statically_establishes_copyability(
        &self,
        context: &SemanticUnitContext,
        ty: TypeId,
    ) -> CheckerQueryResult<bool, Self::UpstreamError>;

    /// Resolves a bound source anchor without exposing its source snapshot.
    ///
    /// The anchor must identify retained source at its bound revision with a valid text range.
    fn source(&self, anchor: BoundSourceAnchor) -> CheckerSource<'_>;

    /// Resolves one declaration syntax anchor from the current immutable compilation snapshot.
    ///
    /// The anchor must identify retained source with a valid text range.
    fn source_syntax(&self, anchor: bray_declarations::SyntaxAnchor) -> CheckerSource<'_>;

    /// Returns the cancellation source for the current request.
    fn cancellation(&self) -> &dyn Cancellation;
}

/// Origin-neutral typed access to one family of symbol-owned semantic queries.
pub trait CheckerSemanticQueryProvider<C>: CheckerRequestContext
where
    C: SymbolQueryContract,
{
    /// Returns the requested immutable semantic value and its owned diagnostics.
    fn resolve_symbol_query(
        &self,
        request: SymbolQueryRequest<C>,
    ) -> CheckerQueryResult<
        Arc<bray_diagnostics::DiagnosticResult<<C as bray_symbols::SymbolQueryContract>::Value>>,
        Self::UpstreamError,
    >;
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_symbols::CallableSignatureQuery;

    use super::{CheckerRequestContext, CheckerSemanticQueryProvider, CheckerSource};

    #[test]
    fn request_context_contracts_are_shareable() {
        fn assert_sync<T: Sync + ?Sized>() {}

        assert_sync::<dyn CheckerRequestContext<UpstreamError = std::convert::Infallible>>();

        assert_sync::<
            dyn CheckerSemanticQueryProvider<
                    CallableSignatureQuery,
                    UpstreamError = std::convert::Infallible,
                >,
        >();
    }

    #[test]
    fn checker_sources_resolve_only_valid_contained_subranges() {
        let source = CheckerSource::new(
            SourceSpan::new(
                SourceId::new(0),
                TextRange::new(TextSize::new(4), TextSize::new(9)),
            ),
            "aébc",
        );

        assert_eq!(
            source.text_for_range(TextRange::new(TextSize::new(5), TextSize::new(8))),
            Some("éb")
        );

        assert_eq!(
            source.text_for_range(TextRange::new(TextSize::new(3), TextSize::new(5))),
            None
        );

        assert_eq!(
            source.text_for_range(TextRange::new(TextSize::new(6), TextSize::new(8))),
            None
        );
    }
}
