use bray_bound_tree::{BoundExpressionId, BoundUnitKey};
use bray_checker::TargetValidityRequest;
use bray_declarations::ModulePartId;
use bray_package_interface::InterfaceSemanticFactKind;
use bray_source::SourceId;
use bray_symbols::{
    AnySymbolId, CallableInstanceId, CallableTypeDirectiveKey, ConstantInstanceKey,
    ConstantValueId, FunctionSymbolId, GenericConstraintObligationKey,
    ImplementationCoherenceDomainKey, ImplementationInstanceId, ImplementationRequirementKey,
    ImplementationSymbolId, ImportedInterfaceId, InterfaceSymbolId, NamedTypeSymbolId,
    SymbolFactKind, TypeId,
};
use bray_target::TargetProfile;

/// The exact artifact-local address of one imported symbol-owned fact category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedSemanticFactKey {
    interface: ImportedInterfaceId,
    owner: InterfaceSymbolId,
    kind: InterfaceSemanticFactKind,
}

impl ImportedSemanticFactKey {
    /// Creates one exact imported semantic-fact address.
    pub const fn new(
        interface: ImportedInterfaceId,
        owner: InterfaceSymbolId,
        kind: InterfaceSemanticFactKind,
    ) -> Self {
        Self {
            interface,
            owner,
            kind,
        }
    }

    /// Returns the loaded interface containing this fact.
    pub const fn interface(self) -> ImportedInterfaceId {
        self.interface
    }

    /// Returns the interface-local symbol that owns this fact.
    pub const fn owner(self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the exact semantic fact category.
    pub const fn kind(self) -> InterfaceSemanticFactKind {
        self.kind
    }
}

/// The exact compilation-local key for one symbol-owned semantic fact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolFactKey {
    symbol: AnySymbolId,
    kind: SymbolFactKind,
}

impl SymbolFactKey {
    /// Creates a key from an exact symbol identity and fact category.
    pub const fn new(symbol: AnySymbolId, kind: SymbolFactKind) -> Self {
        Self { symbol, kind }
    }

    #[cfg(test)]
    pub(crate) const fn symbol(self) -> AnySymbolId {
        self.symbol
    }

    #[cfg(test)]
    pub(crate) const fn kind(self) -> SymbolFactKind {
        self.kind
    }
}

/// The complete compilation-local identity of one concrete constant evaluation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ConstantInstanceFactKey {
    instance: ConstantInstanceKey,
    target: TargetProfile,
}

/// The complete compilation-local identity of one selected constant call.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ConstantCallFactKey {
    callable: CallableInstanceId,
    selected_implementation: Option<ImplementationInstanceId>,
    arguments: std::sync::Arc<[ConstantValueId]>,
    result_type: TypeId,
    target: TargetProfile,
    limits: bray_checker::ConstantEvaluationLimits,
}

/// The exact compilation-local identity of one iteration source selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct IterationSourceFactKey {
    unit: BoundUnitKey,
    expression: BoundExpressionId,
}

impl IterationSourceFactKey {
    pub(crate) const fn new(unit: BoundUnitKey, expression: BoundExpressionId) -> Self {
        Self { unit, expression }
    }

    pub(crate) const fn unit(&self) -> &BoundUnitKey {
        &self.unit
    }

    pub(crate) const fn expression(&self) -> BoundExpressionId {
        self.expression
    }
}

/// The semantic call identity used for dependency-cycle detection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ConstantCallDependencyKey {
    callable: CallableInstanceId,
    selected_implementation: Option<ImplementationInstanceId>,
    arguments: std::sync::Arc<[ConstantValueId]>,
    result_type: TypeId,
    target: TargetProfile,
}

impl ConstantCallFactKey {
    pub(crate) fn new(
        callable: CallableInstanceId,
        selected_implementation: Option<ImplementationInstanceId>,
        arguments: impl Into<std::sync::Arc<[ConstantValueId]>>,
        result_type: TypeId,
        target: TargetProfile,
        limits: bray_checker::ConstantEvaluationLimits,
    ) -> Self {
        Self {
            callable,
            selected_implementation,
            arguments: arguments.into(),
            result_type,
            target,
            limits,
        }
    }

    pub(crate) const fn callable(&self) -> CallableInstanceId {
        self.callable
    }

    pub(crate) const fn selected_implementation(&self) -> Option<ImplementationInstanceId> {
        self.selected_implementation
    }

    pub(crate) fn arguments(&self) -> &[ConstantValueId] {
        &self.arguments
    }

    pub(crate) const fn result_type(&self) -> TypeId {
        self.result_type
    }

    pub(crate) const fn limits(&self) -> bray_checker::ConstantEvaluationLimits {
        self.limits
    }

    pub(crate) fn dependency_key(&self) -> ConstantCallDependencyKey {
        ConstantCallDependencyKey {
            callable: self.callable,
            selected_implementation: self.selected_implementation,
            arguments: std::sync::Arc::clone(&self.arguments),
            result_type: self.result_type,
            // Cycle coordination owns its key independently of the cache entry.
            target: self.target.clone(),
        }
    }
}

impl ConstantInstanceFactKey {
    pub(crate) const fn new(instance: ConstantInstanceKey, target: TargetProfile) -> Self {
        Self { instance, target }
    }
}

/// A compilation-fact identity used only for private dependency coordination.
///
/// The key preserves enough semantic identity to detect dependency cycles and coordinate
/// concurrent requests.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum CompilationFactKey {
    /// The selected target and its target-filtered compiler-known declaration view.
    SelectedTarget,
    /// Post-selection validity of one exact target requirement.
    TargetValidity(TargetValidityRequest),
    /// The selected product and target result for one source module contribution.
    ModuleContributionGate(ModulePartId),
    /// Source-backed directives attached to one callable type occurrence.
    CallableTypeDirectives(CallableTypeDirectiveKey),
    /// The complete canonical compiler-known symbol and fact provider.
    CompilerKnownSymbols,
    /// Deterministic compact identities for bound-unit source anchors.
    BoundUnitIdentities,
    /// One canonical immutable bound unit selected by its exact stable key.
    BoundUnit(BoundUnitKey),
    /// Diagnostics for the current whole-compilation check boundary.
    CheckDiagnostics,
    /// Source constant definitions mapped to their exact expression units.
    ConstantTemplateKeys,
    /// Source callable definitions mapped to their exact body units.
    CallableBodyKeys,
    /// Source predicate definitions mapped to their exact expression units.
    PredicateDefinitionKeys,
    /// One concrete constant value for an exact semantic instance and target profile.
    ConstantInstance(ConstantInstanceFactKey),
    /// One selected constant-call evaluation for exact arguments, target, and limits.
    ConstantCall(ConstantCallFactKey),
    /// The semantic identity shared by nested evaluations of one selected constant call.
    ConstantCallCycle(ConstantCallDependencyKey),
    /// Durable control-flow facts for one bound unit.
    CheckedControlFlow(BoundUnitKey),
    /// Final expression types for one bound unit.
    CheckedExpressionTypes(BoundUnitKey),
    /// Final source-literal values for one bound unit.
    CheckedLiteralValues(BoundUnitKey),
    /// Checked pattern compatibility, binding types, and match coverage for one bound unit.
    CheckedPatterns(BoundUnitKey),
    /// Final semantic selections for one bound unit.
    CheckedSemanticSelections(BoundUnitKey),
    /// Persistent storage identities and occurrence-specific access plans for one bound unit.
    StoragePlan(BoundUnitKey),
    /// Durable last-use and lexical scope-boundary decisions for one bound unit.
    Liveness(BoundUnitKey),
    /// Durable flow-sensitive facts available at checked operation occurrences.
    RefinementFacts(BoundUnitKey),
    /// Checked storage, ownership, movement, and borrow decisions for one bound unit.
    StorageFlowFacts(BoundUnitKey),
    /// Normalized dependency contracts for semantic occurrences in one bound unit.
    DependencyContracts(BoundUnitKey),
    /// Async frame, suspension, task, and cleanup facts for one bound unit.
    AsyncFacts(BoundUnitKey),
    /// Direct callable and runtime-default behavior contributions from one bound unit.
    BodyBehaviorContributions(BoundUnitKey),
    /// Reachable normalized behavior of one checked semantic body.
    CheckedBodyBehavior(BoundUnitKey),
    /// Source-declared value type templates and equality constraints for one bound unit.
    DeclaredValueTypeTemplates(BoundUnitKey),
    /// The private fixed-point computation shared by expression type and selection facts.
    ExpressionSemantics(BoundUnitKey),
    /// The private expression fixed point used before iteration element types are selected.
    ProvisionalExpressionSemantics(BoundUnitKey),
    /// The checked symbolic term produced for one constant definition template.
    SymbolicConstantTerm(BoundUnitKey),
    /// Declaration discovery for one source unit.
    DeclarationChunk(SourceId),
    /// The deterministically merged declaration table.
    DeclarationTable,
    /// Enabled declaration contributions for the selected product and target.
    ProductSourceGraph,
    /// Selected product roots and validated public semantic surface.
    ProductSemantics,
    /// The complete source identity skeleton used while selecting contributions.
    DiscoverySymbolGraph,
    /// Structural validation and identity decoding for one compiled dependency interface.
    DependencyInterface(ImportedInterfaceId),
    /// Diagnostics owned by all selected compiled dependency interfaces.
    ImportedDiagnostics,
    /// The immutable index over available implementation declaration headers.
    ImplementationHeaderIndex,
    /// Uncommitted implementation candidates for one exact requirement.
    ImplementationCandidateSet(ImplementationRequirementKey),
    /// Checked member fulfillment validity for one trait implementation.
    TraitImplementationConformance(ImplementationSymbolId),
    /// Static constraints for one exact generic declaration instance.
    GenericConstraintSatisfaction(GenericConstraintObligationKey),
    /// The selected implementation witness for one exact requirement.
    ImplementationSelection(ImplementationRequirementKey),
    /// The exact protocol operations selected for one iteration source occurrence.
    IterationSource(IterationSourceFactKey),
    /// Decoded and remapped semantic facts for one compiled dependency interface.
    ImportedSemanticGraph(ImportedInterfaceId),
    /// One exact decoded and remapped imported symbol-owned fact category.
    ImportedSemanticFact(ImportedSemanticFactKey),
    /// Implementations participating in one package coherence domain.
    ImplementationParticipation(ImplementationCoherenceDomainKey),
    /// Declaration-level coherence and overload-family validity for the source package.
    ImplementationCoherence,
    /// Declaration-level callable overload-family validity for the source package.
    CallableOverloadValidation,
    /// The validated native boundary contract of one source function.
    ForeignCallableContract(FunctionSymbolId),
    /// Whole-package native symbol identity validity.
    ForeignCallableValidation,
    /// The complete declaration-level member surface of one named type.
    TypeAssociatedSurface(NamedTypeSymbolId),
    /// The checked source-level representation contract of one named type.
    DeclaredTypeRepresentation(NamedTypeSymbolId),
    /// Inherent implementations grouped by their exact named subject.
    TypeAssociatedImplementationIndex,
    /// The deterministic compilation-local imported symbol identity skeleton.
    ImportedSymbolSkeleton,
    /// The current library product's complete immutable interface export bundle.
    PackageInterfaceExportBundle,
    /// The canonical semantic value store for this compilation snapshot.
    SemanticValueStore,
    /// Binding and semantic-analysis diagnostics for the source package.
    SemanticDiagnostics,
    /// Parsed syntax for one source unit.
    SourceUnitSyntax(SourceId),
    /// The deterministic compilation-wide symbol identity graph.
    SymbolGraph,
    /// One symbol-owned semantic fact.
    Symbol(SymbolFactKey),
    /// The syntax tree composed from every loaded source unit.
    SyntaxTree,
}

impl CompilationFactKey {
    pub(crate) const fn bound_unit_key(&self) -> Option<&BoundUnitKey> {
        match self {
            Self::BoundUnit(key)
            | Self::CheckedControlFlow(key)
            | Self::CheckedExpressionTypes(key)
            | Self::CheckedLiteralValues(key)
            | Self::CheckedPatterns(key)
            | Self::CheckedSemanticSelections(key)
            | Self::StoragePlan(key)
            | Self::Liveness(key)
            | Self::RefinementFacts(key)
            | Self::StorageFlowFacts(key)
            | Self::DependencyContracts(key)
            | Self::AsyncFacts(key)
            | Self::BodyBehaviorContributions(key)
            | Self::CheckedBodyBehavior(key)
            | Self::DeclaredValueTypeTemplates(key)
            | Self::ExpressionSemantics(key)
            | Self::ProvisionalExpressionSemantics(key)
            | Self::SymbolicConstantTerm(key) => Some(key),
            Self::IterationSource(key) => Some(key.unit()),
            Self::SelectedTarget
            | Self::TargetValidity(_)
            | Self::ModuleContributionGate(_)
            | Self::CallableTypeDirectives(_)
            | Self::CompilerKnownSymbols
            | Self::BoundUnitIdentities
            | Self::CheckDiagnostics
            | Self::ConstantTemplateKeys
            | Self::CallableBodyKeys
            | Self::PredicateDefinitionKeys
            | Self::ConstantInstance(_)
            | Self::ConstantCall(_)
            | Self::ConstantCallCycle(_)
            | Self::DeclarationChunk(_)
            | Self::DeclarationTable
            | Self::ProductSourceGraph
            | Self::ProductSemantics
            | Self::DiscoverySymbolGraph
            | Self::DependencyInterface(_)
            | Self::ImportedDiagnostics
            | Self::ImplementationHeaderIndex
            | Self::ImplementationCandidateSet(_)
            | Self::TraitImplementationConformance(_)
            | Self::GenericConstraintSatisfaction(_)
            | Self::ImplementationSelection(_)
            | Self::ImportedSemanticGraph(_)
            | Self::ImportedSemanticFact(_)
            | Self::ImplementationParticipation(_)
            | Self::ImplementationCoherence
            | Self::CallableOverloadValidation
            | Self::ForeignCallableContract(_)
            | Self::ForeignCallableValidation
            | Self::TypeAssociatedSurface(_)
            | Self::DeclaredTypeRepresentation(_)
            | Self::TypeAssociatedImplementationIndex
            | Self::ImportedSymbolSkeleton
            | Self::PackageInterfaceExportBundle
            | Self::SemanticValueStore
            | Self::SemanticDiagnostics
            | Self::SourceUnitSyntax(_)
            | Self::SymbolGraph
            | Self::Symbol(_)
            | Self::SyntaxTree => None,
        }
    }
}

impl From<SymbolFactKey> for CompilationFactKey {
    fn from(key: SymbolFactKey) -> Self {
        Self::Symbol(key)
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{AnySymbolId, FunctionSymbolId, SymbolFactKind, SymbolId};

    use super::{
        CompilationFactKey, ConstantCallDependencyKey, ConstantCallFactKey,
        ConstantInstanceFactKey, ImportedSemanticFactKey, SymbolFactKey,
    };

    #[test]
    fn symbol_fact_keys_keep_exact_identity_and_category() {
        let symbol = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(7)));
        let key = SymbolFactKey::new(symbol, SymbolFactKind::CallableSignature);

        assert_eq!(key.symbol(), symbol);
        assert_eq!(key.kind(), SymbolFactKind::CallableSignature);

        assert_eq!(
            CompilationFactKey::from(key),
            CompilationFactKey::Symbol(key)
        );
    }

    #[test]
    fn compilation_fact_keys_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CompilationFactKey>();
        assert_send_sync::<ConstantCallDependencyKey>();
        assert_send_sync::<ConstantCallFactKey>();
        assert_send_sync::<ConstantInstanceFactKey>();
        assert_send_sync::<ImportedSemanticFactKey>();
        assert_send_sync::<SymbolFactKey>();
    }
}
