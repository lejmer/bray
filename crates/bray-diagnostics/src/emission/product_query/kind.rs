/// Exact product operation or identity associated with a failed query.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticProductQueryContext {
    kind: DiagnosticProductQueryContextKind,
    identity: String,
}

impl DiagnosticProductQueryContext {
    /// Creates a product-query context from its typed category and exact structural identity.
    pub fn new(kind: DiagnosticProductQueryContextKind, identity: impl Into<String>) -> Self {
        Self {
            kind,
            identity: identity.into(),
        }
    }

    /// Returns the typed product operation or identity category.
    pub const fn kind(&self) -> DiagnosticProductQueryContextKind {
        self.kind
    }

    /// Returns the exact locale-neutral structural identity retained by the compiler.
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

/// Locale-neutral category of a product operation or identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProductQueryContextKind {
    /// A selected language-level product category.
    Product,
    /// A source function.
    Function,
    /// A semantic symbol.
    Symbol,
    /// A discovered declaration record.
    Declaration,
    /// A declaration container.
    Container,
    /// An exact canonical module path.
    ModulePath,
    /// A stable symbol key.
    SymbolKey,
    /// A source snapshot.
    Source,
    /// A selected target.
    Target,
    /// A concrete code-generation instance.
    Instance,
    /// An exact call site in concrete MIR.
    CallSite,
    /// A code-generation unit.
    CodegenUnit,
    /// A realized code-generation static.
    CodegenStatic,
    /// Exact semantic data for a callable instance.
    CallableData,
    /// An implementation instance.
    Implementation,
    /// An open or closed static reference.
    StaticReference,
    /// A generic substitution.
    Substitution,
    /// A declaration that owns generic parameters.
    GenericOwner,
    /// A semantic type.
    Type,
    /// An implementation requirement.
    ImplementationRequirement,
    /// A MIR unit.
    MirUnit,
    /// A generated MIR helper.
    MirHelper,
    /// A unary compiler-known representation relationship.
    UnaryRepresentation,
    /// An operation in a concrete code-generation instance.
    Operation,
    /// A MIR operation in a stable MIR unit.
    MirOperation,
    /// A callable definition.
    CallableDefinition,
    /// An exact source byte location.
    SourceLocation,
    /// A compiler-known representation role.
    CompilerKnownRepresentation,
    /// A compiler-known declaration.
    CompilerKnownDeclaration,
}

impl DiagnosticProductQueryContextKind {
    /// Returns this context category's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Function => "function",
            Self::Symbol => "symbol",
            Self::Declaration => "declaration",
            Self::Container => "container",
            Self::ModulePath => "module_path",
            Self::SymbolKey => "symbol_key",
            Self::Source => "source",
            Self::Target => "target",
            Self::Instance => "instance",
            Self::CallSite => "call_site",
            Self::CodegenUnit => "codegen_unit",
            Self::CodegenStatic => "codegen_static",
            Self::CallableData => "callable_data",
            Self::Implementation => "implementation",
            Self::StaticReference => "static_reference",
            Self::Substitution => "substitution",
            Self::GenericOwner => "generic_owner",
            Self::Type => "type",
            Self::ImplementationRequirement => "implementation_requirement",
            Self::MirUnit => "mir_unit",
            Self::MirHelper => "mir_helper",
            Self::UnaryRepresentation => "unary_representation",
            Self::Operation => "operation",
            Self::MirOperation => "mir_operation",
            Self::CallableDefinition => "callable_definition",
            Self::SourceLocation => "source_location",
            Self::CompilerKnownRepresentation => "compiler_known_representation",
            Self::CompilerKnownDeclaration => "compiler_known_declaration",
        }
    }
}

/// Locale-neutral category of product data required by a query.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProductDataKind {
    /// A semantic symbol record.
    Symbol,
    /// A test result.
    TestResult,
    /// A declaration's containing module.
    ContainingModule,
    /// A declaration's containing semantic symbol.
    ContainingSymbol,
    /// A declaration member name.
    MemberName,
    /// A declaration source anchor.
    SourceAnchor,
    /// A loaded source snapshot.
    SourceSnapshot,
    /// A stable symbol key.
    SymbolKey,
    /// A concrete code-generation instance.
    ConcreteInstance,
    /// A concrete callable instance.
    CallableInstance,
    /// A concrete anonymous-callable instance.
    AnonymousCallableInstance,
    /// A concrete bound-helper instance.
    BoundHelperInstance,
    /// A concrete generated-lifecycle instance.
    GeneratedLifecycleInstance,
    /// A selected contextual-self witness.
    ContextualSelfWitness,
    /// Retained trait-dispatch metadata.
    TraitDispatch,
    /// A concrete generic substitution.
    GenericSubstitution,
    /// An implementation witness.
    ImplementationWitness,
    /// A callable fulfillment.
    CallableFulfillment,
    /// A generic constraint.
    GenericConstraint,
    /// The declaration that owns generic parameters.
    GenericOwner,
    /// A built-in intrinsic.
    Intrinsic,
    /// A built-in conversion plan.
    ConversionPlan,
    /// A compiler-known representation declaration.
    CompilerKnownRepresentation,
    /// A generated lifecycle role.
    LifecycleRole,
    /// A type-associated lifecycle member.
    LifecycleMember,
    /// A lifecycle helper's semantic type.
    LifecycleType,
    /// A realized static instance.
    RealizedStatic,
    /// A static lifecycle dependency counter.
    StaticDependencyCounter,
    /// A static declaration's resolved type.
    StaticDeclaredType,
    /// A static declaration's initializer.
    StaticInitializer,
    /// An implementation header.
    ImplementationHeader,
    /// A product's discovered test catalog.
    TestDiscovery,
    /// A source's discovered declaration chunk.
    DeclarationChunk,
    /// A discovered declaration's owning container.
    DeclarationContainer,
    /// A declaration container's module path.
    ModulePath,
    /// The product symbol graph's package root.
    PackageRoot,
    /// A module selected by canonical path.
    Module,
    /// A concrete reachability realization.
    ReachabilityRealization,
    /// A completed reachability evaluation.
    ReachabilityEvaluation,
    /// A partition input instance.
    PartitionInstance,
    /// A code-generation unit's mappings.
    CodegenUnitMapping,
    /// The unit that owns a generated product host.
    ProductHostOwnerUnit,
    /// A product-host static mapping.
    ProductHostStaticMapping,
    /// The compiler-known result representation.
    ResultRepresentation,
    /// A callable signature.
    CallableSignature,
    /// A runtime-default query subject.
    RuntimeDefaultSubject,
    /// A MIR unit implementing a runtime default.
    RuntimeDefaultUnit,
    /// A MIR operation result type.
    OperationResultType,
    /// A loaded source's line index.
    SourceLineIndex,
    /// A source location retained by generated code.
    SourceLocation,
    /// A static reference's native-storage contract.
    NativeStaticContract,
    /// A callable's ordered parameter list.
    CallableParameters,
    /// A callable's receiver parameter.
    CallableReceiver,
    /// A static reference's concrete substitution.
    StaticSubstitution,
    /// An imported declaration's external semantic address.
    ImportedSemanticAddress,
    /// A resolved type-expression result.
    ResolvedType,
}

impl DiagnosticProductDataKind {
    /// Returns this product-data category's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::TestResult => "test_result",
            Self::ContainingModule => "containing_module",
            Self::ContainingSymbol => "containing_symbol",
            Self::MemberName => "member_name",
            Self::SourceAnchor => "source_anchor",
            Self::SourceSnapshot => "source_snapshot",
            Self::SymbolKey => "symbol_key",
            Self::ConcreteInstance => "concrete_instance",
            Self::CallableInstance => "callable_instance",
            Self::AnonymousCallableInstance => "anonymous_callable_instance",
            Self::BoundHelperInstance => "bound_helper_instance",
            Self::GeneratedLifecycleInstance => "generated_lifecycle_instance",
            Self::ContextualSelfWitness => "contextual_self_witness",
            Self::TraitDispatch => "trait_dispatch",
            Self::GenericSubstitution => "generic_substitution",
            Self::ImplementationWitness => "implementation_witness",
            Self::CallableFulfillment => "callable_fulfillment",
            Self::GenericConstraint => "generic_constraint",
            Self::GenericOwner => "generic_owner",
            Self::Intrinsic => "intrinsic",
            Self::ConversionPlan => "conversion_plan",
            Self::CompilerKnownRepresentation => "compiler_known_representation",
            Self::LifecycleRole => "lifecycle_role",
            Self::LifecycleMember => "lifecycle_member",
            Self::LifecycleType => "lifecycle_type",
            Self::RealizedStatic => "realized_static",
            Self::StaticDependencyCounter => "static_dependency_counter",
            Self::StaticDeclaredType => "static_declared_type",
            Self::StaticInitializer => "static_initializer",
            Self::ImplementationHeader => "implementation_header",
            Self::TestDiscovery => "test_discovery",
            Self::DeclarationChunk => "declaration_chunk",
            Self::DeclarationContainer => "declaration_container",
            Self::ModulePath => "module_path",
            Self::PackageRoot => "package_root",
            Self::Module => "module",
            Self::ReachabilityRealization => "reachability_realization",
            Self::ReachabilityEvaluation => "reachability_evaluation",
            Self::PartitionInstance => "partition_instance",
            Self::CodegenUnitMapping => "codegen_unit_mapping",
            Self::ProductHostOwnerUnit => "product_host_owner_unit",
            Self::ProductHostStaticMapping => "product_host_static_mapping",
            Self::ResultRepresentation => "result_representation",
            Self::CallableSignature => "callable_signature",
            Self::RuntimeDefaultSubject => "runtime_default_subject",
            Self::RuntimeDefaultUnit => "runtime_default_unit",
            Self::OperationResultType => "operation_result_type",
            Self::SourceLineIndex => "source_line_index",
            Self::SourceLocation => "source_location",
            Self::NativeStaticContract => "native_static_contract",
            Self::CallableParameters => "callable_parameters",
            Self::CallableReceiver => "callable_receiver",
            Self::StaticSubstitution => "static_substitution",
            Self::ImportedSemanticAddress => "imported_semantic_address",
            Self::ResolvedType => "resolved_type",
        }
    }
}

/// Locale-neutral value category required by a product query.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProductValueKind {
    /// A type specialization argument.
    TypeArgument,
    /// A constant specialization argument.
    ConstantArgument,
    /// A trait-satisfaction constraint.
    TraitSatisfactionConstraint,
    /// A predicate generic constraint.
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

impl DiagnosticProductValueKind {
    /// Returns this product-value category's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypeArgument => "type_argument",
            Self::ConstantArgument => "constant_argument",
            Self::TraitSatisfactionConstraint => "trait_satisfaction_constraint",
            Self::PredicateConstraint => "predicate_constraint",
            Self::TypeEqualityConstraint => "type_equality_constraint",
            Self::GenericTypeArgument => "generic_type_argument",
            Self::ClosedStaticReference => "closed_static_reference",
            Self::OpenStaticReference => "open_static_reference",
            Self::NamedType => "named_type",
            Self::CallableType => "callable_type",
            Self::LifecycleRepresentableType => "lifecycle_representable_type",
        }
    }
}
