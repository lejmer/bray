use bray_bound_tree::{BoundExpressionId, BoundUnitKey};
use bray_checker::TargetValidityRequest;
use bray_codegen::{
    BackendArtifactRequest, BackendCapabilityRevision, BackendIdentity, CodegenMappings,
    CodegenOptions, CodegenTarget, CodegenUnitKey,
};
use bray_declarations::ModulePartId;
use bray_linker::LinkerDriverIdentity;
use bray_package_interface::InterfaceSemanticRecordKind;
use bray_runtime_interface::{
    RuntimeArtifactDigest, RuntimeArtifactId, RuntimeArtifactPurpose, RuntimeCapability,
};
use bray_source::SourceId;
use bray_symbols::{
    AnySymbolId, CallableInstanceId, CallableTypeDirectiveKey, ConstantInstanceKey,
    ConstantValueId, FunctionSymbolId, GenericConstraintObligationKey,
    ImplementationCoherenceDomainKey, ImplementationInstanceId, ImplementationRequirementKey,
    ImplementationSymbolId, ImportedInterfaceId, ImportedSemanticAddress, InterfaceSymbolId,
    NamedTypeSymbolId, ProductIdentity, SymbolQueryKind, TypeId,
};
use bray_target::TargetProfile;

/// Exact declaration-owned executable template selected from an imported implementation artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ImportedExecutableTemplateAddress {
    symbol: ImportedSemanticAddress,
    template: bray_ir::MirExecutableTemplateId,
}

impl ImportedExecutableTemplateAddress {
    /// Creates the root executable-template address for one imported declaration.
    pub(crate) const fn root(symbol: ImportedSemanticAddress) -> Self {
        Self::new(symbol, bray_ir::MirExecutableTemplateId::ROOT)
    }

    /// Creates one imported root or nested executable-template address.
    pub(crate) const fn new(
        symbol: ImportedSemanticAddress,
        template: bray_ir::MirExecutableTemplateId,
    ) -> Self {
        Self { symbol, template }
    }

    /// Returns the declaration's imported interface address.
    pub(crate) const fn symbol(self) -> ImportedSemanticAddress {
        self.symbol
    }

    /// Returns the artifact-local template identity.
    pub(crate) const fn template(self) -> bray_ir::MirExecutableTemplateId {
        self.template
    }
}

/// Exact host selections that determine one native product.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct NativeProductQueryKey {
    product: ProductIdentity,
    configuration: crate::BuildConfiguration,
    runtime: Option<std::sync::Arc<[RuntimeComponentQueryIdentity]>>,
    required_capabilities: std::sync::Arc<[RuntimeCapability]>,
    linker_drivers: std::sync::Arc<[LinkerDriverIdentity]>,
}

impl NativeProductQueryKey {
    pub(crate) fn new(
        product: ProductIdentity,
        configuration: crate::BuildConfiguration,
        runtime: Option<std::sync::Arc<[RuntimeComponentQueryIdentity]>>,
        required_capabilities: impl Into<std::sync::Arc<[RuntimeCapability]>>,
        linker_drivers: impl Into<std::sync::Arc<[LinkerDriverIdentity]>>,
    ) -> Self {
        Self {
            product,
            configuration,
            runtime,
            required_capabilities: required_capabilities.into(),
            linker_drivers: linker_drivers.into(),
        }
    }

    pub(crate) const fn product(&self) -> &ProductIdentity {
        &self.product
    }
}

/// Complete immutable identity of one runtime catalog component used by a native product query.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct RuntimeComponentQueryIdentity {
    component: RuntimeArtifactId,
    purpose: RuntimeArtifactPurpose,
    digest: RuntimeArtifactDigest,
    archive: std::path::PathBuf,
}

impl RuntimeComponentQueryIdentity {
    pub(crate) fn new(
        component: RuntimeArtifactId,
        purpose: RuntimeArtifactPurpose,
        digest: RuntimeArtifactDigest,
        archive: std::path::PathBuf,
    ) -> Self {
        Self {
            component,
            purpose,
            digest,
            archive,
        }
    }
}

/// The exact artifact-local address of one imported symbol-owned semantic record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedSemanticRecordKey {
    interface: ImportedInterfaceId,
    owner: InterfaceSymbolId,
    kind: InterfaceSemanticRecordKind,
}

impl ImportedSemanticRecordKey {
    /// Creates one exact imported semantic-record address.
    pub const fn new(
        interface: ImportedInterfaceId,
        owner: InterfaceSymbolId,
        kind: InterfaceSemanticRecordKind,
    ) -> Self {
        Self {
            interface,
            owner,
            kind,
        }
    }

    /// Returns the loaded interface containing this record.
    pub const fn interface(self) -> ImportedInterfaceId {
        self.interface
    }

    /// Returns the interface-local symbol that owns this record.
    pub const fn owner(self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the exact semantic record category.
    pub const fn kind(self) -> InterfaceSemanticRecordKind {
        self.kind
    }
}

/// The exact compilation-local key for one symbol-owned semantic query.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolQueryKey {
    symbol: AnySymbolId,
    kind: SymbolQueryKind,
}

impl SymbolQueryKey {
    /// Creates a key from an exact symbol identity and query category.
    pub const fn new(symbol: AnySymbolId, kind: SymbolQueryKind) -> Self {
        Self { symbol, kind }
    }

    #[cfg(test)]
    pub(crate) const fn symbol(self) -> AnySymbolId {
        self.symbol
    }

    #[cfg(test)]
    pub(crate) const fn kind(self) -> SymbolQueryKind {
        self.kind
    }
}

/// The complete compilation-local identity of one concrete constant evaluation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ConstantInstanceQueryKey {
    instance: ConstantInstanceKey,
    target: TargetProfile,
    limits: bray_checker::ConstantEvaluationLimits,
}

/// The complete compilation-local identity of one selected constant call.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ConstantCallQueryKey {
    callable: CallableInstanceId,
    selected_implementation: Option<ImplementationInstanceId>,
    arguments: std::sync::Arc<[ConstantValueId]>,
    result_type: TypeId,
    target: TargetProfile,
    limits: bray_checker::ConstantEvaluationLimits,
}

/// The exact compilation-local identity of one iteration source selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct IterationSourceQueryKey {
    unit: BoundUnitKey,
    expression: BoundExpressionId,
}

impl IterationSourceQueryKey {
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

/// The exact compilation-local identity of one non-call operation selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct OperationSelectionQueryKey {
    unit: BoundUnitKey,
    expression: BoundExpressionId,
}

/// Complete identity of one independently requested code generation contribution.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct CodegenArtifactQueryKey {
    unit: CodegenUnitKey,
    mappings: CodegenMappings,
    target: CodegenTarget,
    backend: BackendIdentity,
    capability_revision: BackendCapabilityRevision,
    product: bray_symbols::ProductKind,
    options: CodegenOptions,
    artifacts: BackendArtifactRequest,
}

impl CodegenArtifactQueryKey {
    pub(crate) fn new(
        unit: CodegenUnitKey,
        mappings: CodegenMappings,
        target: CodegenTarget,
        backend: BackendIdentity,
        capability_revision: BackendCapabilityRevision,
        product: bray_symbols::ProductKind,
        options: CodegenOptions,
        artifacts: BackendArtifactRequest,
    ) -> Self {
        Self {
            unit,
            mappings,
            target,
            backend,
            capability_revision,
            product,
            options,
            artifacts,
        }
    }

    pub(crate) const fn unit(&self) -> &CodegenUnitKey {
        &self.unit
    }
}

impl OperationSelectionQueryKey {
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

impl ConstantCallQueryKey {
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

impl ConstantInstanceQueryKey {
    pub(crate) const fn new(
        instance: ConstantInstanceKey,
        target: TargetProfile,
        limits: bray_checker::ConstantEvaluationLimits,
    ) -> Self {
        Self {
            instance,
            target,
            limits,
        }
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
    /// The complete canonical compiler-known symbol and semantics provider.
    CompilerKnownSymbols,
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
    ConstantInstance(ConstantInstanceQueryKey),
    /// One selected constant-call evaluation for exact arguments, target, and limits.
    ConstantCall(ConstantCallQueryKey),
    /// The semantic identity shared by nested evaluations of one selected constant call.
    ConstantCallCycle(ConstantCallDependencyKey),
    /// Durable control-flow analysis for one bound unit.
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
    /// Durable flow-sensitive refinements available at checked operation occurrences.
    Refinements(BoundUnitKey),
    /// Checked storage, ownership, movement, and borrow decisions for one bound unit.
    StorageFlow(BoundUnitKey),
    /// Normalized dependency contracts for semantic occurrences in one bound unit.
    DependencyContracts(BoundUnitKey),
    /// Checked compiler-provided memory operations for one bound unit.
    MemoryOperations(BoundUnitKey),
    /// Async frame, suspension, task, and cleanup analysis for one bound unit.
    AsyncAnalysis(BoundUnitKey),
    /// Direct callable and runtime-default behavior contributions from one bound unit.
    BodyBehaviorContributions(BoundUnitKey),
    /// Reachable normalized behavior of one checked semantic body.
    CheckedBodyBehavior(BoundUnitKey),
    /// The lowering result for one exact checked semantic unit.
    LoweredUnit(BoundUnitKey),
    /// One exact backend artifact contribution requested from a code generation unit.
    CodegenArtifact(CodegenArtifactQueryKey),
    /// The complete native product for exact product and host selections.
    NativeProduct(NativeProductQueryKey),
    /// Source-declared value type templates and equality constraints for one bound unit.
    DeclaredValueTypeTemplates(BoundUnitKey),
    /// The private fixed-point computation shared by expression types and semantic selections.
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
    /// Canonical metadata discovery for one exact test package product.
    TestDiscovery(ProductIdentity),
    /// The complete source identity skeleton used while selecting contributions.
    DiscoverySymbolGraph,
    /// Structural validation and identity decoding for one compiled dependency interface.
    DependencyInterface(ImportedInterfaceId),
    /// The implementation payload companion for one compiled dependency interface.
    DependencyImplementation(ImportedInterfaceId),
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
    IterationSource(IterationSourceQueryKey),
    OperationSelection(OperationSelectionQueryKey),
    /// Decoded and remapped semantics for one compiled dependency interface.
    ImportedSemanticGraph(ImportedInterfaceId),
    /// One exact decoded and remapped imported symbol-owned semantic record.
    ImportedSemanticRecord(ImportedSemanticRecordKey),
    /// One checked imported const-callable body selected from an implementation artifact.
    ImportedConstantCallableBody(ImportedSemanticAddress),
    /// One imported executable template selected from an implementation artifact.
    ImportedExecutableTemplate(ImportedExecutableTemplateAddress),
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
    /// Binding and semantic-analysis diagnostics for the source package.
    SemanticDiagnostics,
    /// Parsed syntax for one source unit.
    SourceUnitSyntax(SourceId),
    /// Semantic source references grouped by target for one source unit.
    SourceReferenceIndex(SourceId),
    /// The deterministic compilation-wide symbol identity graph.
    SymbolGraph,
    /// One symbol-owned semantic query.
    Symbol(SymbolQueryKey),
    /// The syntax tree composed from every loaded source unit.
    SyntaxTree,
}

impl CompilationFactKey {
    pub(crate) const fn has_stable_snapshot_identity(&self) -> bool {
        !matches!(
            self,
            Self::TargetValidity(_)
                | Self::ModuleContributionGate(_)
                | Self::CallableTypeDirectives(_)
                | Self::ConstantInstance(_)
                | Self::ConstantCall(_)
                | Self::ConstantCallCycle(_)
                | Self::ImplementationCandidateSet(_)
                | Self::TraitImplementationConformance(_)
                | Self::GenericConstraintSatisfaction(_)
                | Self::ImplementationSelection(_)
                | Self::ImplementationParticipation(_)
                | Self::ForeignCallableContract(_)
                | Self::TypeAssociatedSurface(_)
                | Self::DeclaredTypeRepresentation(_)
                | Self::Symbol(_)
        )
    }

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
            | Self::Refinements(key)
            | Self::StorageFlow(key)
            | Self::DependencyContracts(key)
            | Self::MemoryOperations(key)
            | Self::AsyncAnalysis(key)
            | Self::BodyBehaviorContributions(key)
            | Self::CheckedBodyBehavior(key)
            | Self::LoweredUnit(key)
            | Self::DeclaredValueTypeTemplates(key)
            | Self::ExpressionSemantics(key)
            | Self::ProvisionalExpressionSemantics(key)
            | Self::SymbolicConstantTerm(key) => Some(key),
            Self::IterationSource(key) => Some(key.unit()),
            Self::OperationSelection(key) => Some(key.unit()),
            Self::SelectedTarget
            | Self::TargetValidity(_)
            | Self::ModuleContributionGate(_)
            | Self::CallableTypeDirectives(_)
            | Self::CompilerKnownSymbols
            | Self::CheckDiagnostics
            | Self::ConstantTemplateKeys
            | Self::CallableBodyKeys
            | Self::PredicateDefinitionKeys
            | Self::NativeProduct(_)
            | Self::ConstantInstance(_)
            | Self::ConstantCall(_)
            | Self::ConstantCallCycle(_)
            | Self::CodegenArtifact(_)
            | Self::DeclarationChunk(_)
            | Self::DeclarationTable
            | Self::ProductSourceGraph
            | Self::ProductSemantics
            | Self::TestDiscovery(_)
            | Self::DiscoverySymbolGraph
            | Self::DependencyInterface(_)
            | Self::DependencyImplementation(_)
            | Self::ImportedDiagnostics
            | Self::ImplementationHeaderIndex
            | Self::ImplementationCandidateSet(_)
            | Self::TraitImplementationConformance(_)
            | Self::GenericConstraintSatisfaction(_)
            | Self::ImplementationSelection(_)
            | Self::ImportedSemanticGraph(_)
            | Self::ImportedSemanticRecord(_)
            | Self::ImportedConstantCallableBody(_)
            | Self::ImportedExecutableTemplate(_)
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
            | Self::SemanticDiagnostics
            | Self::SourceUnitSyntax(_)
            | Self::SourceReferenceIndex(_)
            | Self::SymbolGraph
            | Self::Symbol(_)
            | Self::SyntaxTree => None,
        }
    }
}

impl From<SymbolQueryKey> for CompilationFactKey {
    fn from(key: SymbolQueryKey) -> Self {
        Self::Symbol(key)
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{AnySymbolId, FunctionSymbolId, SymbolId, SymbolQueryKind};

    use super::{
        CompilationFactKey, ConstantCallDependencyKey, ConstantCallQueryKey,
        ConstantInstanceQueryKey, ImportedSemanticRecordKey, SymbolQueryKey,
    };

    #[test]
    fn symbol_semantic_keys_keep_exact_identity_and_category() {
        let symbol = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(7)));
        let key = SymbolQueryKey::new(symbol, SymbolQueryKind::CallableSignature);

        assert_eq!(key.symbol(), symbol);
        assert_eq!(key.kind(), SymbolQueryKind::CallableSignature);

        assert_eq!(
            CompilationFactKey::from(key),
            CompilationFactKey::Symbol(key)
        );
    }

    #[test]
    fn compilation_semantic_keys_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CompilationFactKey>();
        assert_send_sync::<ConstantCallDependencyKey>();
        assert_send_sync::<ConstantCallQueryKey>();
        assert_send_sync::<ConstantInstanceQueryKey>();
        assert_send_sync::<ImportedSemanticRecordKey>();
        assert_send_sync::<SymbolQueryKey>();
    }
}
