use std::sync::Arc;

use bray_base::Cancellation;
use bray_bound_tree::{
    AnyBoundNodeId, AsyncAnalysisBuildError, BorrowCapabilityId, BoundBlockId,
    BoundDependencyContractId, BoundExpressionId, BoundPatternId, BoundSourceAnchor, BoundUnit,
    BoundUnitId, BoundUnitKind, DependencyContractsBuildError, SemanticSelectionTableBuildError,
    SemanticSnapshotBuildError, StorageAccessId, StorageFlowBuildError, StorageIdentityId,
    StorageOperationStatus,
};
use bray_compiler_known::{ImplementationHook, RepresentationRole};
use bray_diagnostics::DiagnosticResult;
use bray_source::{SourceId, SourceSpan, SourceVersion, TextRange, TextSize};
use bray_symbols::{
    AnyLocalSymbolId, AnySymbolId, AvailableCompilerKnownSymbols, ConstantTermId,
    DeclaredTypeRepresentation, GenericConstraintObligationKey, ImplementationRequirementKey,
    ImplementationSelection, LocalBindingSymbolId, MemberLookupResult, NamedTypeSymbolId,
    ProofOutcome, SemanticValueStore, StructSymbol, StructSymbolId, SymbolGraph, SymbolKey,
    SymbolName, SymbolQueryContract, SymbolQueryKind, SymbolQueryRequest, TraitApplicationId,
    TraitTypeMemberSymbolId, TypeId, UnionPayloadFieldSymbol, UnionPayloadFieldSymbolId,
    UnionSymbol, UnionSymbolId, UnionVariantSymbol, UnionVariantSymbolId,
};
use bray_target::TargetProfile;

use crate::{CheckedConstantTermsBuildError, CheckerUnitViewError, SemanticUnitContext};

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
    /// A required semantic query could not be supplied.
    SemanticQueryUnavailable {
        /// The exact symbol that owns the query.
        symbol: AnySymbolId,
        /// The unavailable query category.
        kind: SymbolQueryKind,
    },
    /// Canonical semantic value construction or lookup failed without an available store cause.
    SemanticValueUnavailable,
    /// The canonical semantic value store rejected a construction or lookup operation.
    SemanticValueStore(bray_symbols::SemanticValueStoreError),
    /// The representation type for an atomic value is unavailable.
    AtomicRepresentationTypeUnavailable,
    /// The representation arguments for an atomic value are unavailable.
    AtomicRepresentationArgumentsUnavailable,
    /// The atomic initializer argument has no available compile-time value.
    AtomicInitializerArgumentUnavailable,
    /// The atomic initializer result cannot be retained as a compile-time value.
    AtomicInitializerResultUnavailable,
    /// The uninitialized-storage initializer result cannot be retained as a compile-time value.
    UninitInitializerResultUnavailable,
    /// An imported native operation does not match its compiled definition.
    ImportedExecutableTemplateMismatch,
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
    /// One correlated checker input belongs to another bound unit.
    IncompatibleInput {
        /// The input table whose identity disagreed with the requested unit.
        input: CheckerInputKind,
        /// Requested bound unit identity.
        expected_unit: BoundUnitId,
        /// Requested bound unit category.
        expected_kind: BoundUnitKind,
        /// Input table's bound unit identity.
        actual_unit: BoundUnitId,
        /// Input table's bound unit category.
        actual_kind: BoundUnitKind,
    },
    /// Checked constant occurrences could not form one unambiguous term table.
    CheckedConstantTerms(CheckedConstantTermsBuildError),
    /// Literal-value table construction rejected one exact input relationship.
    LiteralValue(CheckerLiteralValueFailure),
    /// Pattern-checking input construction retained conflicting evidence.
    PatternInput(CheckerPatternInputFailure),
    /// Constant-evaluation input construction retained conflicting evidence.
    ConstantInput(CheckerConstantInputFailure),
    /// Constant evaluation encountered an invalid source-correlated input.
    ConstantEvaluation(CheckerConstantEvaluationFailure),
    /// Semantic-selection inputs do not describe the requested bound unit or operation category.
    InvalidSemanticSelectionInput,
    /// Construction of the final semantic-selection table rejected one exact relationship.
    SemanticSelection(SemanticSelectionTableBuildError),
    /// Constant-evaluation inputs do not describe the requested bound unit.
    InvalidConstantEvaluationInput,
    /// Storage-planning inputs or constructed records violate the requested unit contract.
    InvalidStoragePlan,
    /// Liveness inputs or durable decisions violate the requested unit contract.
    InvalidLiveness,
    /// Refinement inputs do not describe the requested bound unit.
    InvalidRefinementInput,
    /// Refinement resource counts cannot be represented by the diagnostic protocol.
    RefinementCapacityUnrepresentable,
    /// Host allocation failed while constructing refinement analysis storage.
    RefinementStorageUnavailable,
    /// One exact storage-flow contract was violated.
    StorageFlow(CheckerStorageFlowFailure),
    /// Storage-flow analysis produced a state forbidden for one exact planned source operation.
    InvalidStorageOperation {
        /// Source expression whose planned access produced the forbidden state.
        expression: BoundExpressionId,
        /// Planned storage access whose state was rejected.
        access: StorageAccessId,
        /// Exact rejected storage state.
        status: StorageOperationStatus,
    },
    /// Correlated body-semantic inputs or durable results violate the requested unit contract.
    InvalidBodySemantics,
    /// Correlated semantic results describe different bound units or unit categories.
    SemanticSnapshot(SemanticSnapshotBuildError),
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

/// Identifies one correlated semantic input supplied to a checker service.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerInputKind {
    /// Checked asynchronous behavior.
    AsyncAnalysis,
    /// Checked control-flow structure.
    ControlFlow,
    /// Declaration-provided value type templates.
    DeclaredValueTypes,
    /// Complete checked expression semantics.
    ExpressionSemantics,
    /// Checked expression types.
    ExpressionTypes,
    /// Checked literal values.
    LiteralValues,
    /// Checked memory operations.
    MemoryOperations,
    /// Checked pattern semantics.
    Patterns,
    /// Checked semantic selections.
    SemanticSelections,
    /// Planned storage operations.
    StoragePlan,
}

impl CheckerInputKind {
    /// Returns this input category's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AsyncAnalysis => "async_analysis",
            Self::ControlFlow => "control_flow",
            Self::DeclaredValueTypes => "declared_value_types",
            Self::ExpressionSemantics => "expression_semantics",
            Self::ExpressionTypes => "expression_types",
            Self::LiteralValues => "literal_values",
            Self::MemoryOperations => "memory_operations",
            Self::Patterns => "patterns",
            Self::SemanticSelections => "semantic_selections",
            Self::StoragePlan => "storage_plan",
        }
    }
}

/// One exact literal-value table contract violation retained by the checker boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerLiteralValueFailure {
    /// The expression-type table belongs to another unit.
    ForeignExpressionTypes,
    /// An entry does not name a literal expression.
    InvalidLiteral {
        /// The invalid literal expression identity.
        expression: BoundExpressionId,
    },
    /// A literal expression has no checked type.
    MissingExpressionType {
        /// The literal expression without a checked type.
        expression: BoundExpressionId,
    },
    /// A literal expression has no checked value.
    MissingLiteralValue {
        /// The literal expression without a checked value.
        expression: BoundExpressionId,
    },
    /// A literal value has a type different from its expression.
    ValueTypeMismatch {
        /// The literal expression whose value has another type.
        expression: BoundExpressionId,
    },
    /// More than one value was supplied for one literal expression.
    DuplicateExpression {
        /// The repeated literal expression identity.
        expression: BoundExpressionId,
    },
}

/// Conflicting evidence retained while constructing one pattern-checking request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerPatternInputFailure {
    /// One pattern has conflicting declared type templates.
    ConflictingDeclaredPattern {
        /// The pattern with conflicting declared types.
        pattern: BoundPatternId,
    },
    /// One pattern has conflicting constant evidence.
    ConflictingConstantPattern {
        /// The pattern with conflicting constants.
        pattern: BoundPatternId,
    },
    /// One guard expression has conflicting constant values.
    ConflictingGuard {
        /// The guard expression with conflicting constants.
        expression: BoundExpressionId,
    },
}

/// Conflicting evidence retained while constructing one constant-evaluation request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerConstantInputFailure {
    /// One reference expression has conflicting resolutions.
    ConflictingReference {
        /// The reference expression with conflicting resolutions.
        expression: BoundExpressionId,
    },
    /// One local constant has conflicting symbolic terms.
    ConflictingLocalTerm {
        /// The local constant with conflicting terms.
        local: AnyLocalSymbolId,
    },
}

/// One invalid source-correlated input encountered during constant evaluation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerConstantEvaluationFailure {
    /// The requested expression root is absent from the unit.
    InvalidExpressionRoot {
        /// The unavailable expression root.
        expression: BoundExpressionId,
    },
    /// The requested block root is absent from the unit.
    InvalidBlockRoot {
        /// The unavailable block root.
        block: BoundBlockId,
    },
    /// The selected expression has no checked type.
    MissingExpressionType {
        /// The expression without a checked type.
        expression: BoundExpressionId,
    },
    /// A block-rooted request has no declared result type.
    MissingBlockResultType {
        /// The block without a result type.
        block: BoundBlockId,
    },
    /// Evaluation reached an expression absent from the unit.
    MissingExpression {
        /// The unavailable expression identity.
        expression: BoundExpressionId,
    },
    /// Evaluation reached a block absent from the unit.
    MissingBlock {
        /// The unavailable block identity.
        block: BoundBlockId,
    },
    /// Pattern evaluation was requested without checked pattern input.
    MissingPatternInput {
        /// The pattern that required checked input.
        pattern: BoundPatternId,
    },
    /// A requested pattern is absent from the unit or checked pattern input.
    MissingPattern {
        /// The unavailable pattern identity.
        pattern: BoundPatternId,
    },
    /// A requested pattern binding has no checked projection.
    MissingPatternBinding {
        /// The binding without a checked pattern projection.
        binding: LocalBindingSymbolId,
    },
    /// Closed evaluation unexpectedly retained a propagating term.
    UnexpectedPropagation {
        /// The symbolic term that unexpectedly propagated.
        term: ConstantTermId,
    },
}

/// The exact storage-flow contract violated by checker inputs or constructed analysis.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerStorageFlowFailure {
    /// One analysis input belongs to a different checked source body.
    IncompatibleInput {
        /// Input table whose identity disagreed with the requested source body.
        input: StorageFlowInputKind,
        /// Requested source body identity.
        expected_unit: BoundUnitId,
        /// Requested source body category.
        expected_kind: BoundUnitKind,
        /// Input table's source body identity.
        actual_unit: BoundUnitId,
        /// Input table's source body category.
        actual_kind: BoundUnitKind,
    },
    /// Durable storage-flow construction rejected an exact invariant.
    FlowConstruction(StorageFlowBuildError),
    /// A selected call or iteration produced a dependency contract for another source body.
    ForeignDependencyContract,
    /// Durable dependency-contract construction rejected an exact invariant.
    DependencyContractsConstruction(DependencyContractsBuildError),
    /// Durable async-analysis construction rejected an exact invariant.
    AsyncConstruction(AsyncAnalysisBuildError),
    /// A non-recovered await expression has no selected dependency contract.
    MissingAwaitDependencyContract { expression: BoundExpressionId },
    /// An await expression names a dependency contract absent from its source body.
    MissingDependencyContract {
        expression: BoundExpressionId,
        contract: BoundDependencyContractId,
    },
    /// The callable signature and callable type disagree about their parameter count.
    CallableParameterCountMismatch {
        callable: AnySymbolId,
        signature_parameters: usize,
        type_parameters: usize,
    },
    /// A callable declaration's resolved type is not callable.
    CallableTypeNotCallable { callable: AnySymbolId },
    /// A storage borrow identity has no retained capability.
    MissingBorrowCapability { borrow: BorrowCapabilityId },
    /// A control-flow exit has no retained source origin.
    MissingExitOrigin { exit: AnyBoundNodeId },
    /// A control-flow transfer names a block absent from its source body.
    MissingBlock { block: BoundBlockId },
    /// A planned storage access is absent from its source body's storage plan.
    MissingStorageAccess { access: StorageAccessId },
    /// A planned storage identity is absent from its source body's storage plan.
    MissingStorageIdentity { identity: StorageIdentityId },
    /// A storage identity refers to a declaration whose name is unavailable.
    MissingStorageSymbolName { symbol: AnySymbolId },
    /// Walking a source body ended with unbalanced lexical scopes.
    UnbalancedScopes { open_scope: Option<BoundBlockId> },
    /// A pattern referenced while assigning lexical ownership is absent from its source body.
    MissingPattern { pattern: BoundPatternId },
}

/// Identifies one correlated input to storage-flow-related analysis.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StorageFlowInputKind {
    ExpressionTypes,
    SemanticSelections,
    StoragePlan,
    Liveness,
    Refinements,
    MemoryOperations,
    StorageFlow,
    DependencyContracts,
}

/// A failure while requesting a checker dependency.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckerQueryError<Upstream = std::convert::Infallible> {
    /// Cancellation was observed while obtaining the dependency.
    Cancelled,
    /// Compiler infrastructure could not supply the dependency.
    Infrastructure(CheckerInfrastructureError),
    /// The coordinating query layer returned one of its own exact failures.
    Upstream(Upstream),
}

impl CheckerQueryError {
    /// Widens a checker-local failure to a boundary with an upstream error type.
    pub fn with_upstream<Upstream>(self) -> CheckerQueryError<Upstream> {
        match self {
            Self::Cancelled => CheckerQueryError::Cancelled,
            Self::Infrastructure(error) => CheckerQueryError::Infrastructure(error),
            Self::Upstream(error) => match error {},
        }
    }
}

impl<Upstream> From<CheckerInfrastructureError> for CheckerQueryError<Upstream> {
    fn from(error: CheckerInfrastructureError) -> Self {
        Self::Infrastructure(error)
    }
}

/// The result of requesting one checker dependency.
pub type CheckerQueryResult<T, Upstream = std::convert::Infallible> =
    Result<T, CheckerQueryError<Upstream>>;

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

    /// Returns whether semantic context exactly describes the supplied bound unit.
    fn semantic_context_matches(&self, unit: &BoundUnit, context: &SemanticUnitContext) -> bool;

    /// Returns the canonical semantic values used by bound structure and queries.
    fn semantic_values(&self) -> &SemanticValueStore;

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

    /// Returns the target atomic representation selected for one concrete plain-storage type.
    fn plain_storage_atomic_representation(
        &self,
        _ty: TypeId,
    ) -> CheckerQueryResult<Option<bray_target::TargetAtomicRepresentation>, Self::UpstreamError>
    {
        Ok(None)
    }

    /// Returns whether a declared type has finalization or destruction behavior.
    fn declared_type_has_lifecycle(
        &self,
        subject: NamedTypeSymbolId,
    ) -> CheckerQueryResult<DiagnosticResult<bool>, Self::UpstreamError>;

    /// Returns whether the enclosing static context establishes a copy contract for an open type.
    fn statically_establishes_copyability(
        &self,
        context: &SemanticUnitContext,
        ty: TypeId,
    ) -> CheckerQueryResult<bool, Self::UpstreamError>;

    /// Resolves a bound source anchor without exposing its source snapshot.
    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError>;

    /// Resolves one declaration syntax anchor from the current immutable compilation snapshot.
    fn source_syntax(
        &self,
        anchor: bray_declarations::SyntaxAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError>;

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
