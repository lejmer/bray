use bray_codegen::{BackendSelectionError, CodegenCallSite, CodegenInstanceKey, CodegenUnitKey};
use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_declarations::{ContainerId, DeclarationId};
use bray_ir::MirOperationId;
use bray_source::{SourceId, TextSize, TextSizeOverflow};
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, ConstantValueId, ConstantValueKind,
    FunctionSymbolId, GenericOwnerId, GenericSubstitutionId, ImplementationInstanceId,
    ImplementationRequirementKey, ModuleOwnerId, ModulePathKey, PackageIdentity, ProductIdentity,
    ProductKind, StaticReferenceSelection, SymbolKey, TypeId,
};

/// Exact product-specialization or realization query failure.
///
/// Retains bounded construction, configured backend and external artifact failures.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProductQueryError {
    cause: Box<ProductQueryFailure>,
}

impl ProductQueryError {
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
    #[track_caller]
    fn from(cause: ProductQueryFailure) -> Self {
        if let ProductQueryFailure::GenericSubstitution { cause: shape, .. } = &cause {
            assert!(
                matches!(
                    shape,
                    bray_symbols::GenericSubstitutionShapeError::OrdinalOverflow
                ),
                "product substitution construction failed: {cause:?}"
            );
        }

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
    /// Product construction for one concrete code generation instance.
    Instance(CodegenInstanceKey),
    /// Product construction for one exact MIR call site.
    CallSite(CodegenCallSite),
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
    /// The concrete substitution applied to a static reference.
    StaticSubstitution,
    /// The external semantic address of an imported declaration.
    ImportedSemanticAddress,
    /// A type-expression template resolved to one semantic type.
    ResolvedType,
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
    /// A named semantic type.
    NamedType,
    /// A callable semantic type.
    CallableType,
}

/// One typed product-specialization or realization contract failure.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ProductQueryFailure {
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
    /// Test discovery was requested for a product incompatible with this compilation.
    TestProductMismatch {
        /// The exact requested product.
        requested: ProductIdentity,
        /// The package owned by this compilation.
        compilation_package: PackageIdentity,
        /// The selected compilation product kind.
        compilation_kind: ProductKind,
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
    #[track_caller]
    pub(crate) fn missing(context: ProductQueryContext, data: ProductDataKind) -> Self {
        panic!("product missing contract failed: context={context:?}, data={data:?}")
    }

    /// Creates a value-category failure for one exact product context.
    #[track_caller]
    pub(crate) fn unexpected_kind(
        context: ProductQueryContext,
        expected: ProductValueKind,
        actual: ProductValueKind,
    ) -> Self {
        panic!(
            "product unexpected_kind contract failed: context={context:?}, expected={expected:?}, actual={actual:?}"
        )
    }

    /// Creates a count-contract failure for one exact product context.
    #[track_caller]
    pub(crate) fn count_mismatch(
        context: ProductQueryContext,
        data: ProductDataKind,
        expected: usize,
        actual: usize,
    ) -> Self {
        panic!(
            "product count_mismatch contract failed: context={context:?}, data={data:?}, expected={expected}, actual={actual}"
        )
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
