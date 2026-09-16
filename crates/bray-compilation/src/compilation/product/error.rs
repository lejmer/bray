use bray_codegen::{
    BackendSelectionError, CodegenCallSite, CodegenImplementationWitness, CodegenInstanceKey,
    CodegenStaticInstanceKey, CodegenTarget, CodegenUnitKey,
};
use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_declarations::{ContainerId, DeclarationId};
use bray_ir::{
    MirCallTarget, MirGeneratedLifecycleRole, MirHelperReference, MirOperationId, MirOperationKind,
    MirUnitKey,
};
use bray_package_interface::InterfaceNativeBoundaryKind;
use bray_runtime_interface::ExecutableEntryResult;
use bray_source::{SourceId, SourceVersion, TextSize, TextSizeOverflow};
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, ConstantValueId, ConstantValueKind,
    FunctionSymbolId, GenericOwnerId, GenericSubstitutionId, ImplementationInstanceId,
    ImplementationRequirementKey, ImplementationSelection, ModuleOwnerId, ModulePathKey,
    PackageIdentity, ProductIdentity, ProductKind, ProofOutcome, StaticReferenceSelection,
    SymbolKey, SymbolKind, TraitApplicationId, TraitSymbolId, TypeId,
};

/// Stable category for an exact product-specialization or realization failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ProductQueryErrorKind {
    /// Required product data was unavailable.
    MissingData,
    /// A retained value violated an exact product contract.
    ContractViolation,
    /// Generic specialization or implementation selection was inconsistent.
    Specialization,
    /// Product coordination or lifecycle ordering failed.
    Coordination,
    /// Source or external identity construction failed.
    Identity,
}

/// Exact product-specialization or realization query failure.
///
/// The stable category is public while exact compiler identities remain private to the compilation
/// that owns them.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProductQueryError {
    cause: Box<ProductQueryFailure>,
}

impl ProductQueryError {
    /// Returns the stable product-query failure category.
    pub fn kind(&self) -> ProductQueryErrorKind {
        self.cause.kind()
    }

    pub(crate) fn cause(&self) -> &ProductQueryFailure {
        &self.cause
    }
}

impl std::fmt::Display for ProductQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "compiler product-query failure: {:?}",
            self.cause()
        )
    }
}

impl std::error::Error for ProductQueryError {}

impl From<ProductQueryFailure> for ProductQueryError {
    fn from(cause: ProductQueryFailure) -> Self {
        Self {
            cause: Box::new(cause),
        }
    }
}

/// One exact product-specialization or realization operation that failed.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ProductQueryContext {
    /// Product-wide construction for one selected language-level product kind.
    Product(ProductKind),
    /// Product construction for one source function.
    Function(FunctionSymbolId),
    /// Product construction for one semantic symbol.
    Symbol(AnySymbolId),
    Declaration(DeclarationId),
    Container(ContainerId),
    /// Product construction for one exact module lookup.
    ModulePath {
        /// The package or module owning the path.
        owner: ModuleOwnerId,
        /// The exact canonical module path.
        path: ModulePathKey,
    },
    /// Product construction for one stable symbol key.
    SymbolKey(SymbolKey),
    /// Product construction for one source snapshot.
    Source(SourceId),
    /// Product construction for one selected target.
    Target(CodegenTarget),
    /// Product construction for one concrete code generation instance.
    Instance(CodegenInstanceKey),
    /// Product construction for one exact MIR call site.
    CallSite(CodegenCallSite),
    /// Product construction for one partitioned code generation unit.
    CodegenUnit(CodegenUnitKey),
    /// Product construction for one realized code generation static.
    CodegenStatic(CodegenStaticInstanceKey),
    /// Product construction for one resolved callable instance value.
    CallableData(CallableInstanceData),
    /// Product construction for one implementation instance.
    Implementation(ImplementationInstanceId),
    /// Product construction for one open or closed static reference.
    StaticReference(StaticReferenceSelection),
    /// Product construction for one generic substitution.
    Substitution(GenericSubstitutionId),
    /// Product construction for one generic declaration owner.
    GenericOwner(GenericOwnerId),
    /// Product construction for one semantic type.
    Type(TypeId),
    /// Product construction for one implementation requirement.
    ImplementationRequirement(ImplementationRequirementKey),
    /// Product construction for one MIR unit.
    MirUnit(MirUnitKey),
    /// Product construction for one generated MIR helper.
    MirHelper(MirHelperReference),
    /// Product construction for one operation in a concrete code generation instance.
    Operation {
        /// The instance containing the operation.
        instance: CodegenInstanceKey,
        /// The exact MIR operation.
        operation: MirOperationId,
    },
    /// Product construction for one callable definition.
    CallableDefinition(CallableDefinitionId),
    /// Product construction for one exact source location.
    SourceLocation {
        /// The source containing the location.
        source: SourceId,
        /// The UTF-8 byte offset inside the source.
        offset: TextSize,
    },
    /// Product construction for one unary compiler-known representation application.
    UnaryRepresentation {
        /// The compiler-known representation role.
        role: bray_compiler_known::RepresentationRole,
        /// The exact semantic argument type.
        argument: TypeId,
    },
    /// Product construction for one compiler-known representation role.
    CompilerKnownRepresentation(bray_compiler_known::RepresentationRole),
    /// Product construction for one compiler-known declaration.
    CompilerKnownDeclaration(CompilerKnownDeclarationKey),
}

/// One required product-specialization or realization value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ProductDataKind {
    /// A semantic symbol record.
    Symbol,
    /// A test result associated with a test function.
    TestResult,
    /// The logical module containing a declaration.
    ContainingModule,
    /// The semantic symbol containing a declaration.
    ContainingSymbol,
    /// A declaration member name.
    MemberName,
    /// A declaration source anchor.
    SourceAnchor,
    /// A loaded source snapshot.
    SourceSnapshot,
    /// A stable symbol identity key.
    SymbolKey,
    /// A concrete code generation instance.
    ConcreteInstance,
    /// A semantic callable instance retained by one concrete code generation instance.
    CallableInstance,
    /// A concrete anonymous-callable instance.
    AnonymousCallableInstance,
    /// A concrete bound-helper instance.
    BoundHelperInstance,
    /// A concrete generated-lifecycle instance.
    GeneratedLifecycleInstance,
    /// A selected contextual-self witness.
    ContextualSelfWitness,
    /// Trait dispatch metadata retained by a demanded call.
    TraitDispatch,
    /// A concrete generic substitution.
    GenericSubstitution,
    /// The generic owner of a declaration.
    GenericOwner,
    /// An implementation witness.
    ImplementationWitness,
    /// A callable fulfillment supplied by an implementation.
    CallableFulfillment,
    /// A generic constraint selected by ordinal.
    GenericConstraint,
    /// A built-in intrinsic selected for a call.
    Intrinsic,
    /// A built-in conversion plan.
    ConversionPlan,
    /// A compiler-known representation declaration.
    CompilerKnownRepresentation,
    /// A generated lifecycle role.
    LifecycleRole,
    /// The semantic type owned by a lifecycle helper.
    LifecycleType,
    /// A realized static instance.
    RealizedStatic,
    /// A static lifecycle dependency counter.
    StaticDependencyCounter,
    /// A static declaration's initializer.
    StaticInitializer,
    /// An implementation's checked header.
    ImplementationHeader,
    /// Product test-discovery output.
    TestDiscovery,
    /// One source's discovered declaration chunk.
    DeclarationChunk,
    /// The container owning one discovered declaration.
    DeclarationContainer,
    /// A declaration container's module path.
    ModulePath,
    /// The package root of the product symbol graph.
    PackageRoot,
    /// A module selected by canonical path.
    Module,
    /// One demanded reachability realization.
    ReachabilityRealization,
    /// One demanded reachability evaluation.
    ReachabilityEvaluation,
    /// A concrete instance retained by one partition.
    PartitionInstance,
    /// Code generation mappings for one partitioned unit.
    CodegenUnitMapping,
    /// The semantic owner unit for a product host entry.
    ProductHostOwnerUnit,
    /// The code generation static mapped to a product host entry.
    ProductHostStaticMapping,
    /// The runtime representation of a result value.
    ResultRepresentation,
    /// A callable signature.
    CallableSignature,
    /// The subject of a runtime-default query.
    RuntimeDefaultSubject,
    /// The MIR unit implementing a runtime default.
    RuntimeDefaultUnit,
    /// The result type of a MIR operation.
    OperationResultType,
    /// The line index for a loaded source.
    SourceLineIndex,
    /// A source location retained by generated code.
    SourceLocation,
    /// The native-storage contract for a static reference.
    NativeStaticContract,
    /// A callable's ordered parameter list.
    CallableParameters,
    /// A callable's receiver parameter.
    CallableReceiver,
    /// The concrete substitution applied to a static reference.
    StaticSubstitution,
    /// The external semantic address of an imported declaration.
    ImportedSemanticAddress,
    /// A type-expression template resolved to one semantic type.
    ResolvedType,
    /// The completed compilation diagnostic set requested by product emission.
    CompilationDiagnostics,
    /// The package-interface contribution requested by product emission.
    PackageInterfaceContribution,
    /// The planned package-implementation artifact requested by product emission.
    PackageImplementationArtifact,
}

/// One exact value category expected by product specialization or realization.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ProductValueKind {
    /// A type specialization argument.
    TypeArgument,
    /// A constant specialization argument.
    ConstantArgument,
    /// A trait-satisfaction generic constraint.
    TraitSatisfactionConstraint,
    /// A constant-predicate generic constraint.
    PredicateConstraint,
    /// A type-equality generic constraint.
    TypeEqualityConstraint,
    /// A type-valued generic argument.
    GenericTypeArgument,
    /// A closed static reference.
    ClosedStaticReference,
    /// An open static reference.
    OpenStaticReference,
    /// A named semantic type.
    NamedType,
    /// A callable semantic type.
    CallableType,
    /// A semantic type with represented lifecycle work.
    LifecycleRepresentableType,
}

/// One typed product-specialization or realization contract failure.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ProductQueryFailure {
    /// Required product data is unavailable for the exact owning context.
    Missing {
        /// The exact product operation or identity that required the data.
        context: ProductQueryContext,
        /// The unavailable product data category.
        data: ProductDataKind,
    },
    /// A product value has a category incompatible with its exact owning context.
    UnexpectedKind {
        /// The exact product operation or identity that received the value.
        context: ProductQueryContext,
        /// The required value category.
        expected: ProductValueKind,
        /// The actual value category.
        actual: ProductValueKind,
    },
    /// A product collection has a count incompatible with its exact owning context.
    CountMismatch {
        /// The exact product operation or identity that owns the collection.
        context: ProductQueryContext,
        /// The counted product data category.
        data: ProductDataKind,
        /// The required number of values.
        expected: usize,
        /// The actual number of values.
        actual: usize,
    },
    /// Independently produced product data conflicts for one exact owning context.
    Conflict {
        /// The exact product operation or identity that owns the values.
        context: ProductQueryContext,
        /// The conflicting product data category.
        data: ProductDataKind,
    },
    /// A constant value category cannot participate in structural specialization identity.
    UnsupportedConstantValue {
        /// The exact constant value rejected by identity construction.
        value: ConstantValueId,
        /// The retained constant value category.
        kind: ConstantValueKind,
    },
    /// A generic substitution violated its exact ordered parameter and argument contract.
    GenericSubstitution {
        /// The substitution being specialized when reconstruction failed, when already interned.
        substitution: Option<GenericSubstitutionId>,
        /// The exact rejected substitution relationship.
        cause: bray_symbols::GenericSubstitutionShapeError,
    },
    /// A selected implementation witness describes another trait application.
    TraitApplicationMismatch {
        /// The exact implementation witness whose requirement was inspected.
        witness: ImplementationInstanceId,
        /// The trait application required by the call.
        expected: TraitApplicationId,
        /// The trait application supplied by the witness.
        actual: TraitApplicationId,
    },
    /// Two implementation instances map to the same code generation witness identity.
    ConflictingImplementationWitness {
        /// The colliding stable code generation witness identity.
        identity: CodegenImplementationWitness,
        /// The first implementation instance assigned to the identity.
        existing: ImplementationInstanceId,
        /// The conflicting implementation instance.
        actual: ImplementationInstanceId,
    },
    /// A semantic type has a data shape incompatible with the required product operation.
    UnexpectedSemanticType {
        /// The exact semantic type whose data was inspected.
        ty: TypeId,
        /// The required type category.
        expected: ProductValueKind,
        /// The exact retained semantic type data.
        actual: bray_symbols::TypeData,
    },
    /// A product coordination lock was poisoned while retaining cached immutable results.
    SynchronizationPoisoned {
        /// The exact product coordination component whose lock was poisoned.
        component: ProductSynchronizationComponent,
    },
    /// A constant owner requires more outgoing records than the native descriptor can express.
    OutgoingCapacityOverflow { value: ConstantValueId },
    /// A static lifecycle dependency counter exceeded its representable range.
    StaticDependencyOverflow {
        /// The exact static instance whose incoming dependency count overflowed.
        static_instance: CodegenStaticInstanceKey,
    },
    /// A static lifecycle dependency counter was decremented below zero.
    StaticDependencyUnderflow {
        /// The exact static instance whose incoming dependency count underflowed.
        static_instance: CodegenStaticInstanceKey,
    },
    /// Static lifecycle dependencies contain a cycle among the retained instances.
    StaticLifecycleCycle {
        /// The exact static instances left after deterministic topological ordering stopped.
        instances: Box<[CodegenStaticInstanceKey]>,
    },
    /// Two concrete realizations use one stable code generation identity.
    ConflictingConcreteInstance {
        /// The colliding stable code generation instance identity.
        key: CodegenInstanceKey,
    },
    /// Two static relocations use one constant-value leaf.
    ConflictingStaticRelocation {
        /// The colliding constant value.
        value: ConstantValueId,
    },
    /// A concrete instance retained a callable different from its required definition.
    CallableDefinitionMismatch {
        /// The exact concrete code generation instance.
        instance: CodegenInstanceKey,
        /// The required callable definition.
        expected: CallableDefinitionId,
        /// The callable definition retained by the instance.
        actual: CallableDefinitionId,
    },
    /// A selected trait context belongs to a different trait definition.
    TraitDefinitionMismatch {
        /// The exact product operation or identity that owns the relationship.
        context: ProductQueryContext,
        /// The required trait definition.
        expected: TraitSymbolId,
        /// The selected trait definition.
        actual: TraitSymbolId,
    },
    /// A loaded source cannot be assigned a valid code generation file identity.
    InvalidCodegenSourceFile {
        /// The loaded source identity, when one was available.
        source: Option<SourceId>,
        /// The rejected source path.
        path: String,
    },
    /// A source line index exceeded the compiler's compact text-offset range.
    SourceIndex {
        /// The exact loaded source.
        source: SourceId,
        /// The exact rejected byte count.
        cause: TextSizeOverflow,
    },
    /// Stable external-symbol identity construction failed for one semantic symbol.
    ExternalSymbolIdentity {
        /// The exact semantic symbol being exported.
        symbol: AnySymbolId,
        /// The exact package-interface export failure.
        cause: Box<crate::compilation::PackageInterfaceExportError>,
    },
    /// A compiler-known declaration key literal is invalid.
    InvalidCompilerKnownDeclarationKey {
        /// The rejected declaration key.
        key: String,
    },
    /// A recognized standard-library declaration key literal is invalid.
    InvalidRecognizedStandardLibraryDeclarationKey {
        /// The rejected declaration key.
        key: String,
    },
    /// A package identity literal is invalid.
    InvalidPackageIdentity {
        /// The rejected package identity.
        identity: String,
    },
    /// An executable entry produced a result shape unsupported by the selected host contract.
    UnexpectedEntryResult {
        /// The unsupported entry result shape.
        actual: ExecutableEntryResult,
    },
    /// A semantic type maps to a different compiler-known representation than required.
    CompilerKnownRepresentationMismatch {
        /// The exact semantic type whose representation was inspected.
        ty: TypeId,
        /// The required compiler-known representation.
        expected: bray_compiler_known::RepresentationRole,
        /// The representation retained by the type, when compiler-known.
        actual: Option<bray_compiler_known::RepresentationRole>,
    },
    /// An imported native static reference resolved to another native-boundary kind.
    NativeBoundaryKindMismatch {
        /// The exact static reference being realized.
        reference: StaticReferenceSelection,
        /// The exact imported boundary kind.
        actual: InterfaceNativeBoundaryKind,
    },
    /// A semantic symbol has a category incompatible with the required product operation.
    UnexpectedSymbolKind {
        /// The exact semantic symbol.
        symbol: AnySymbolId,
        /// The required symbol category.
        expected: SymbolKind,
        /// The actual symbol category.
        actual: SymbolKind,
    },
    /// A stable symbol key used as an implementation witness is not an implementation declaration.
    ImplementationSymbolKeyExpected {
        /// The exact rejected stable symbol key.
        key: SymbolKey,
        /// The exact rejected symbol category.
        actual: SymbolKind,
    },
    /// A generated helper encountered an operation incompatible with its helper role.
    InvalidHelperOperation {
        /// The exact instance and MIR operation being analyzed.
        context: ProductQueryContext,
        /// The generated helper being analyzed.
        helper: MirHelperReference,
        /// The exact incompatible MIR operation.
        operation: MirOperationKind,
    },
    /// A cleanup operation must call one immediate, directly selected callable.
    InvalidCleanupCallTarget {
        context: ProductQueryContext,
        target: MirCallTarget,
    },
    /// A generated lifecycle instance and its demanded helper disagree on lifecycle role.
    LifecycleRoleMismatch {
        /// The exact concrete code generation instance.
        instance: CodegenInstanceKey,
        /// The lifecycle role retained by the instance template.
        expected: Option<MirGeneratedLifecycleRole>,
        /// The lifecycle role implied by the demanded helper.
        actual: Option<MirGeneratedLifecycleRole>,
    },
    /// A built-in trait proof did not establish one demanded implementation requirement.
    BuiltInProofMismatch {
        /// The exact demanded implementation requirement.
        requirement: ImplementationRequirementKey,
        /// The exact proof outcome, when one was available.
        actual: Option<ProofOutcome>,
    },
    /// Implementation selection did not produce the required selected witness.
    ImplementationSelectionMismatch {
        /// The exact demanded implementation requirement.
        requirement: ImplementationRequirementKey,
        /// The exact selection result.
        actual: ImplementationSelection,
    },
    /// An imported runtime-default provider belongs to an unsupported semantic subject.
    UnsupportedRuntimeDefaultSubject {
        /// The exact runtime-default provider.
        provider: AnySymbolId,
        /// The exact unsupported subject.
        subject: AnySymbolId,
    },
    /// A semantic symbol cannot serve as a callable definition.
    InvalidCallableDefinitionSymbol {
        /// The exact rejected semantic symbol.
        symbol: AnySymbolId,
        /// The exact rejected symbol category.
        actual: SymbolKind,
    },
    /// A loaded source snapshot does not match the version retained by generated MIR.
    SourceSnapshotMismatch {
        /// The exact source identity.
        source: SourceId,
        /// The required source version.
        expected: SourceVersion,
        /// The loaded source version, when the source was available.
        actual: Option<SourceVersion>,
    },
    /// Test discovery was requested for a product incompatible with this compilation.
    TestProductMismatch {
        /// The exact requested product.
        requested: ProductIdentity,
        /// The package owned by this compilation.
        compilation_package: PackageIdentity,
        /// The selected compilation product kind.
        compilation_kind: ProductKind,
    },
    /// Canonical test-catalog construction rejected exact test metadata.
    TestCatalog {
        /// The exact selected product.
        product: ProductIdentity,
        /// The exact rejected test identity.
        identity: bray_test_protocol::TestIdentity,
        /// The exact catalog contract category.
        cause: ProductTestCatalogFailureKind,
    },
    /// A stable test error-type identity could not be constructed from a structural digest.
    InvalidTestErrorTypeIdentity {
        /// The exact structural type digest.
        digest: [u8; 32],
    },
    /// A discovered module path cannot be represented as a canonical symbol path.
    InvalidModulePath {
        /// The exact declaration whose container supplied the path.
        declaration: DeclarationId,
        /// The exact rejected path segments.
        segments: Box<[String]>,
    },
    /// An entry result type has no runtime representation supported by static finalization.
    UnsupportedEntryResultType {
        /// The exact semantic result type.
        ty: TypeId,
        /// Its compiler-known representation, when one exists.
        actual: Option<bray_compiler_known::RepresentationRole>,
    },
    /// Backend selection failed for one partitioned code generation unit.
    CodegenBackendSelection {
        /// The exact partitioned code generation unit.
        unit: CodegenUnitKey,
        /// The exact backend-selection failure.
        cause: BackendSelectionError,
    },
}

impl ProductQueryFailure {
    /// Creates a missing-data failure for one exact product context.
    pub(crate) const fn missing(context: ProductQueryContext, data: ProductDataKind) -> Self {
        Self::Missing { context, data }
    }

    /// Creates a value-category failure for one exact product context.
    pub(crate) const fn unexpected_kind(
        context: ProductQueryContext,
        expected: ProductValueKind,
        actual: ProductValueKind,
    ) -> Self {
        Self::UnexpectedKind {
            context,
            expected,
            actual,
        }
    }

    /// Creates a count-contract failure for one exact product context.
    pub(crate) const fn count_mismatch(
        context: ProductQueryContext,
        data: ProductDataKind,
        expected: usize,
        actual: usize,
    ) -> Self {
        Self::CountMismatch {
            context,
            data,
            expected,
            actual,
        }
    }

    const fn kind(&self) -> ProductQueryErrorKind {
        match self {
            Self::Missing { .. } => ProductQueryErrorKind::MissingData,
            Self::GenericSubstitution { .. }
            | Self::TraitApplicationMismatch { .. }
            | Self::ConflictingImplementationWitness { .. } => {
                ProductQueryErrorKind::Specialization
            }
            Self::SynchronizationPoisoned { .. }
            | Self::OutgoingCapacityOverflow { .. }
            | Self::StaticDependencyOverflow { .. }
            | Self::StaticDependencyUnderflow { .. }
            | Self::StaticLifecycleCycle { .. } => ProductQueryErrorKind::Coordination,
            Self::UnsupportedConstantValue { .. }
            | Self::InvalidCodegenSourceFile { .. }
            | Self::SourceIndex { .. }
            | Self::ExternalSymbolIdentity { .. }
            | Self::InvalidCompilerKnownDeclarationKey { .. }
            | Self::InvalidRecognizedStandardLibraryDeclarationKey { .. }
            | Self::InvalidPackageIdentity { .. } => ProductQueryErrorKind::Identity,
            Self::UnexpectedKind { .. }
            | Self::CountMismatch { .. }
            | Self::Conflict { .. }
            | Self::UnexpectedSemanticType { .. }
            | Self::ConflictingConcreteInstance { .. }
            | Self::ConflictingStaticRelocation { .. }
            | Self::CallableDefinitionMismatch { .. }
            | Self::TraitDefinitionMismatch { .. }
            | Self::UnexpectedEntryResult { .. }
            | Self::CompilerKnownRepresentationMismatch { .. }
            | Self::NativeBoundaryKindMismatch { .. }
            | Self::UnexpectedSymbolKind { .. }
            | Self::ImplementationSymbolKeyExpected { .. }
            | Self::InvalidHelperOperation { .. }
            | Self::InvalidCleanupCallTarget { .. }
            | Self::LifecycleRoleMismatch { .. }
            | Self::BuiltInProofMismatch { .. }
            | Self::ImplementationSelectionMismatch { .. } => {
                ProductQueryErrorKind::ContractViolation
            }
            Self::UnsupportedRuntimeDefaultSubject { .. } => {
                ProductQueryErrorKind::ContractViolation
            }
            Self::InvalidCallableDefinitionSymbol { .. } => {
                ProductQueryErrorKind::ContractViolation
            }
            Self::SourceSnapshotMismatch { .. } => ProductQueryErrorKind::Identity,
            Self::TestProductMismatch { .. }
            | Self::TestCatalog { .. }
            | Self::InvalidTestErrorTypeIdentity { .. }
            | Self::InvalidModulePath { .. }
            | Self::UnsupportedEntryResultType { .. } => ProductQueryErrorKind::ContractViolation,
            Self::CodegenBackendSelection { .. } => ProductQueryErrorKind::ContractViolation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ProductTestCatalogFailureKind {
    ProductMismatch,
    DuplicateIdentity,
    InvalidResultMetadata,
}

impl From<ProductQueryFailure> for crate::compilation::CodegenPreparationError {
    fn from(error: ProductQueryFailure) -> Self {
        crate::fact::FactQueryError::from(error).into()
    }
}

/// One synchronized product-query component.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ProductSynchronizationComponent {
    /// Cached lifecycle requirements indexed by semantic type.
    LifecycleNeeds,
}
