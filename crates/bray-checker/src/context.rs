use std::sync::Arc;

use bray_base::Cancellation;
use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, BoundSourceAnchor, BoundUnit};
use bray_compiler_known::{ImplementationHook, RepresentationRole};
use bray_diagnostics::DiagnosticResult;
use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
use bray_symbols::{
    AnySymbolId, AvailableCompilerKnownSymbols, DeclaredTypeRepresentation,
    GenericConstraintObligationKey, ImplementationRequirementKey, ImplementationSelection,
    MemberLookupResult, NamedTypeSymbolId, ProofOutcome, SemanticValueStore, StructSymbol,
    StructSymbolId, SymbolFactContract, SymbolFactKind, SymbolFactRequest, SymbolFactResult,
    SymbolGraph, SymbolKey, SymbolName, TraitApplicationId, TraitTypeMemberSymbolId, TypeId,
    UnionPayloadFieldSymbol, UnionPayloadFieldSymbolId, UnionSymbol, UnionSymbolId,
    UnionVariantSymbol, UnionVariantSymbolId,
};
use bray_target::TargetProfile;

use crate::{CheckerUnitViewError, SemanticUnitContext};

/// A checker infrastructure failure that is neither a source diagnostic nor cancellation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerInfrastructureError {
    /// The compilation does not contain the source named by a bound anchor.
    MissingSource {
        /// The unavailable source identity.
        source_id: SourceId,
    },
    /// A bound anchor names a different source revision than the compilation.
    SourceVersionMismatch {
        /// The source whose revision did not match.
        source_id: SourceId,
        /// The revision retained by the bound anchor.
        expected: SourceVersion,
        /// The revision available in the compilation.
        actual: SourceVersion,
    },
    /// A bound anchor does not cover a valid UTF-8 range in its source revision.
    InvalidSourceRange {
        /// The invalid source span.
        span: SourceSpan,
    },
    /// A required semantic fact could not be supplied.
    SemanticFactUnavailable {
        /// The exact symbol that owns the fact.
        symbol: AnySymbolId,
        /// The unavailable fact category.
        kind: SymbolFactKind,
    },
    /// Canonical semantic value construction or lookup failed.
    SemanticValueUnavailable,
    /// A required compiler-known representation is unavailable for the selected target.
    CompilerKnownRepresentationUnavailable {
        /// The unavailable representation role.
        role: RepresentationRole,
    },
    /// Type-checking input names an expression outside the requested unit.
    InvalidExpressionTypeInput {
        /// The invalid expression identity.
        expression: BoundExpressionId,
    },
    /// Semantic-selection inputs do not describe the requested bound unit or operation category.
    InvalidSemanticSelectionInput,
    /// Literal-value inputs do not describe the requested bound unit.
    InvalidLiteralValueInput,
    /// Constant-evaluation inputs do not describe the requested bound unit.
    InvalidConstantEvaluationInput,
    /// Pattern-checking inputs disagree for one bound occurrence.
    InvalidPatternCheckInput,
    /// Storage-planning inputs or constructed records violate the requested unit contract.
    InvalidStoragePlan,
    /// Liveness inputs or durable decisions violate the requested unit contract.
    InvalidLivenessFacts,
    /// Refinement inputs do not describe the requested bound unit.
    InvalidRefinementInput,
    /// Storage-flow inputs or durable decisions violate the requested unit contract.
    InvalidStorageFlowFacts,
    /// A committed bound relationship names a node absent from the requested unit.
    InvalidBoundNode {
        /// The missing bound node identity.
        node: AnyBoundNodeId,
    },
    /// One unit contains more expression variables than the checker can identify compactly.
    ExpressionTypeCapacityExceeded,
    /// A checker unit view did not match its canonical bound unit.
    InvalidUnitView(CheckerUnitViewError),
}

/// A failure while requesting a checker dependency.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerFactError {
    /// Cancellation was observed while obtaining the dependency.
    Cancelled,
    /// Compiler infrastructure could not supply the dependency.
    Infrastructure(CheckerInfrastructureError),
}

impl From<CheckerInfrastructureError> for CheckerFactError {
    fn from(error: CheckerInfrastructureError) -> Self {
        Self::Infrastructure(error)
    }
}

/// The result of requesting one checker dependency.
pub type CheckerFactResult<T> = Result<T, CheckerFactError>;

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
    /// Returns whether semantic context exactly describes the supplied bound unit.
    fn semantic_context_matches(&self, unit: &BoundUnit, context: &SemanticUnitContext) -> bool;

    /// Returns the canonical semantic values used by bound structure and facts.
    fn semantic_values(&self) -> &SemanticValueStore;

    /// Returns the compilation-wide symbol graph.
    fn symbols(&self) -> &SymbolGraph;

    /// Returns the stable semantic key of a source or imported declaration.
    fn symbol_key(&self, symbol: AnySymbolId) -> CheckerFactResult<Option<&SymbolKey>> {
        Ok(self.symbols().symbol_key(symbol))
    }

    /// Resolves one ordinary member from a source or imported declaration.
    fn lookup_member(
        &self,
        owner: AnySymbolId,
        name: &str,
    ) -> CheckerFactResult<MemberLookupResult<AnySymbolId>> {
        Ok(self.symbols().lookup_member(owner, name))
    }

    /// Returns the ordinary name of a source or imported declaration member.
    fn member_name(&self, member: AnySymbolId) -> CheckerFactResult<Option<&SymbolName>> {
        Ok(self.symbols().member_name(member))
    }

    /// Returns a source or imported structure declaration.
    fn structure(&self, id: StructSymbolId) -> CheckerFactResult<Option<&StructSymbol>> {
        Ok(self.symbols().structure(id))
    }

    /// Returns a source or imported union declaration.
    fn union(&self, id: UnionSymbolId) -> CheckerFactResult<Option<&UnionSymbol>> {
        Ok(self.symbols().union(id))
    }

    /// Returns a source or imported union variant declaration.
    fn union_variant(
        &self,
        id: UnionVariantSymbolId,
    ) -> CheckerFactResult<Option<&UnionVariantSymbol>> {
        Ok(self.symbols().union_variant(id))
    }

    /// Returns a source or imported union payload field declaration.
    fn union_payload_field(
        &self,
        id: UnionPayloadFieldSymbolId,
    ) -> CheckerFactResult<Option<&UnionPayloadFieldSymbol>> {
        Ok(self.symbols().union_payload_field(id))
    }

    /// Returns compiler-known symbols available for the current target.
    fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols;

    /// Resolves a compiler-known or recognized standard-library implementation hook.
    fn implementation_hook(
        &self,
        symbol: AnySymbolId,
    ) -> CheckerFactResult<Option<ImplementationHookResolution>> {
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
    ) -> CheckerFactResult<Option<ImplementationHookResolution>> {
        Ok(None)
    }

    /// Returns the selected language-level target profile.
    fn selected_target(&self) -> &TargetProfile;

    /// Checks one source constant expression embedded in a type template.
    fn checked_constant_expression(
        &self,
        occurrence: bray_symbols::ConstantExpressionOccurrence,
    ) -> CheckerFactResult<DiagnosticResult<bray_symbols::ConstantTermId>>;

    /// Proves the static constraints for one exact generic declaration instance.
    fn generic_constraints(
        &self,
        obligation: GenericConstraintObligationKey,
    ) -> CheckerFactResult<DiagnosticResult<ProofOutcome>>;

    /// Selects the implementation satisfying one exact subject and trait application.
    fn implementation_selection(
        &self,
        _requirement: ImplementationRequirementKey,
    ) -> CheckerFactResult<DiagnosticResult<ImplementationSelection>> {
        Ok(DiagnosticResult::without_diagnostics(
            ImplementationSelection::Unavailable,
        ))
    }

    /// Resolves one selected type-valued member projection when its witness is available.
    fn selected_type_valued_member(
        &self,
        _subject: TypeId,
        _application: TraitApplicationId,
        _member: TraitTypeMemberSymbolId,
    ) -> CheckerFactResult<DiagnosticResult<Option<TypeId>>> {
        Ok(DiagnosticResult::without_diagnostics(None))
    }

    /// Returns the checked representation contract for one declared type.
    fn declared_type_representation(
        &self,
        subject: NamedTypeSymbolId,
    ) -> CheckerFactResult<DiagnosticResult<DeclaredTypeRepresentation>>;

    /// Returns whether a declared type has finalization or destruction behavior.
    fn declared_type_has_lifecycle(
        &self,
        subject: NamedTypeSymbolId,
    ) -> CheckerFactResult<DiagnosticResult<bool>>;

    /// Returns whether the enclosing static context establishes a copy contract for an open type.
    fn statically_establishes_copyability(
        &self,
        context: &SemanticUnitContext,
        ty: TypeId,
    ) -> CheckerFactResult<bool>;

    /// Resolves a bound source anchor without exposing its source snapshot.
    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError>;

    /// Returns the cancellation source for the current request.
    fn cancellation(&self) -> &dyn Cancellation;
}

/// Origin-neutral typed access to one family of symbol-owned semantic facts.
pub trait CheckerSemanticFactProvider<C>: CheckerRequestContext
where
    C: SymbolFactContract,
{
    /// Returns the requested immutable semantic fact and its owned diagnostics.
    fn symbol_fact(
        &self,
        request: SymbolFactRequest<C>,
    ) -> CheckerFactResult<Arc<SymbolFactResult<C>>>;
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_symbols::CallableSignatureFact;

    use super::{CheckerRequestContext, CheckerSemanticFactProvider, CheckerSource};

    #[test]
    fn request_context_contracts_are_shareable() {
        fn assert_sync<T: Sync + ?Sized>() {}

        assert_sync::<dyn CheckerRequestContext>();
        assert_sync::<dyn CheckerSemanticFactProvider<CallableSignatureFact>>();
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
