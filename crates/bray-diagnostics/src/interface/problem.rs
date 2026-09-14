macro_rules! define_interface_symbol_kinds {
    ($($variant:ident => $key:literal, $documentation:literal;)+) => {
        /// Closed semantic symbol categories retained by package-interface diagnostics.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum DiagnosticInterfaceSymbolKind {
            $(#[doc = $documentation] $variant,)+
        }

        impl DiagnosticInterfaceSymbolKind {
            /// Returns the stable machine key for this symbol category.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $key,)+
                }
            }
        }
    };
}

define_interface_symbol_kinds! {
    CompilerKnownEnvironment => "compiler_known_environment", "The compiler-provided declaration root.";
    Package => "package", "A package root.";
    Module => "module", "A logical module.";
    TrustedCapability => "trusted_capability", "A compiler-known trusted capability.";
    Constant => "constant", "A constant declaration.";
    Static => "static", "A static storage declaration.";
    Function => "function", "A function declaration.";
    Predicate => "predicate", "A predicate declaration.";
    CallableContract => "callable_contract", "A callable contract declaration.";
    CallableOverload => "callable_overload", "A callable overload family.";
    ImplementationOverload => "implementation_overload", "An implementation overload family.";
    Struct => "struct", "A structure declaration.";
    Union => "union", "A union declaration.";
    Trait => "trait", "A trait declaration.";
    InherentImplementation => "inherent_implementation", "An inherent implementation.";
    UnnamedTraitImplementation => "unnamed_trait_implementation", "An unnamed trait implementation.";
    NamedTraitImplementation => "named_trait_implementation", "A named trait implementation.";
    StructField => "struct_field", "A structure field.";
    UnionVariant => "union_variant", "A union variant.";
    UnionPayloadField => "union_payload_field", "A union payload field.";
    TypeCallableMember => "type_callable_member", "A callable type member.";
    Constructor => "constructor", "A constructor.";
    Finalizer => "finalizer", "A finalizer.";
    Destructor => "destructor", "A destructor.";
    ScopeEnter => "scope_enter", "A scope-enter lifecycle operation.";
    ScopeExit => "scope_exit", "A scope-exit lifecycle operation.";
    InherentTypeMember => "inherent_type_valued_member", "An inherent type-valued member.";
    TraitCallableMember => "trait_callable_member", "A trait callable requirement.";
    TraitConstantMember => "trait_constant_member", "A trait constant requirement.";
    TraitTypeMember => "trait_type_valued_member", "A trait type-valued requirement.";
    TraitPredicateMember => "trait_predicate_member", "A trait predicate requirement.";
    TraitFinalizerRequirement => "trait_finalizer_requirement", "A trait finalizer requirement.";
    TraitDestructorRequirement => "trait_destructor_requirement", "A trait destructor requirement.";
    TraitScopeEnterRequirement => "trait_scope_enter_requirement", "A trait scope-enter requirement.";
    TraitScopeExitRequirement => "trait_scope_exit_requirement", "A trait scope-exit requirement.";
    TraitCallableFulfillment => "trait_callable_fulfillment", "A callable trait fulfillment.";
    TraitConstantFulfillment => "trait_constant_fulfillment", "A constant trait fulfillment.";
    TraitTypeFulfillment => "trait_type_valued_fulfillment", "A type-valued trait fulfillment.";
    TraitPredicateFulfillment => "trait_predicate_fulfillment", "A predicate trait fulfillment.";
    TraitScopeEnterFulfillment => "trait_scope_enter_fulfillment", "A scope-enter trait fulfillment.";
    TraitScopeExitFulfillment => "trait_scope_exit_fulfillment", "A scope-exit trait fulfillment.";
    GenericTypeParameter => "generic_type_parameter", "A generic type parameter.";
    GenericConstParameter => "generic_const_parameter", "A generic constant parameter.";
    CallableParameter => "callable_parameter", "A callable parameter.";
    PredicateParameter => "predicate_parameter", "A predicate parameter.";
    ReceiverParameter => "receiver_parameter", "A compiler-introduced receiver parameter.";
    CallableParameterDefaultProvider => "callable_parameter_default_provider", "A callable-parameter default provider.";
    StructFieldDefaultProvider => "struct_field_default_provider", "A structure-field default provider.";
    UnionPayloadDefaultProvider => "union_payload_default_provider", "A union-payload default provider.";
    LocalBinding => "local_binding", "A body-local binding.";
    LocalConstant => "local_constant", "A body-local constant.";
    AnonymousCallable => "anonymous_callable", "An anonymous callable.";
    AnonymousCallableParameter => "anonymous_callable_parameter", "An anonymous-callable parameter.";
    PostconditionResult => "postcondition_result", "A postcondition result binding.";
}

/// Closed imported-symbol relationship categories retained by artifact diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceRelationshipKind {
    /// A package-to-module relationship.
    PackageModule,
    /// A module-to-declaration relationship.
    ModuleMember,
    /// A type-to-member relationship.
    TypeMember,
    /// A trait-to-requirement relationship.
    TraitMember,
    /// An implementation-to-member relationship.
    ImplementationMember,
    /// A structure-to-field relationship.
    StructField,
    /// A union-to-variant relationship.
    UnionVariant,
    /// A variant-to-payload-field relationship.
    UnionPayloadField,
    /// A declaration-to-generic-parameter relationship.
    GenericParameter,
    /// A callable-to-parameter relationship.
    CallableParameter,
    /// A predicate-to-parameter relationship.
    PredicateParameter,
    /// An overload-family-to-arm relationship.
    OverloadArm,
    /// An implementation-to-trait-fulfillment relationship.
    ImplementationFulfillment,
    /// A declaration-to-default-provider relationship.
    DefaultProvider,
}

impl DiagnosticInterfaceRelationshipKind {
    /// Returns the stable machine key for this relationship category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PackageModule => "package_module",
            Self::ModuleMember => "module_member",
            Self::TypeMember => "type_member",
            Self::TraitMember => "trait_member",
            Self::ImplementationMember => "implementation_member",
            Self::StructField => "struct_field",
            Self::UnionVariant => "union_variant",
            Self::UnionPayloadField => "union_payload_field",
            Self::GenericParameter => "generic_parameter",
            Self::CallableParameter => "callable_parameter",
            Self::PredicateParameter => "predicate_parameter",
            Self::OverloadArm => "overload_arm",
            Self::ImplementationFulfillment => "implementation_fulfillment",
            Self::DefaultProvider => "default_provider",
        }
    }
}

/// Closed semantic value-table categories retained by artifact diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSemanticValueKind {
    /// Canonical semantic types.
    Type,
    /// Evaluated constant values.
    ConstantValue,
    /// Checked constant terms.
    ConstantTerm,
    /// Ordered generic substitutions.
    GenericSubstitution,
    /// Applied traits.
    TraitApplication,
    /// Substituted callable definitions.
    CallableInstance,
    /// Substituted implementation definitions.
    ImplementationInstance,
    /// Portable dependency contracts.
    DependencyContractTemplate,
}

impl DiagnosticSemanticValueKind {
    /// Returns the stable machine key for this semantic value category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::ConstantValue => "constant_value",
            Self::ConstantTerm => "constant_term",
            Self::GenericSubstitution => "generic_substitution",
            Self::TraitApplication => "trait_application",
            Self::CallableInstance => "callable_instance",
            Self::ImplementationInstance => "implementation_instance",
            Self::DependencyContractTemplate => "dependency_contract_template",
        }
    }
}

/// Closed owner-relative identity used by an interface declaration key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceDeclarationIdentity {
    /// The declaration is identified by its ordinary semantic name.
    Name(String),
    /// The declaration is identified by its stable owner-relative ordinal.
    Ordinal(u32),
}

/// Closed synthesized-symbol role with its required ordinal shape encoded by the variant.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceSynthesizedIdentity {
    /// Implicit receiver parameter.
    ReceiverParameter,
    /// Written generic type parameter at the contained ordinal.
    DeclaredGenericTypeParameter(u32),
    /// Written generic constant parameter at the contained ordinal.
    DeclaredGenericConstParameter(u32),
    /// Written callable parameter at the contained ordinal.
    CallableParameter(u32),
    /// Written predicate parameter at the contained ordinal.
    PredicateParameter(u32),
    /// Inferred implementation type parameter at the contained ordinal.
    InferredImplementationTypeParameter(u32),
    /// Inferred implementation constant parameter at the contained ordinal.
    InferredImplementationConstParameter(u32),
    /// Callable-parameter default provider.
    CallableParameterDefaultProvider,
    /// Structure-field default provider.
    StructFieldDefaultProvider,
    /// Union-payload default provider.
    UnionPayloadDefaultProvider,
}

/// Exact recursive semantic identity retained from an imported or compiler-owned symbol key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceSymbolIdentity {
    /// Compiler-provided semantic environment root.
    CompilerKnownEnvironment,
    /// Package root with its stable package identity.
    Package(String),
    /// Logical module beneath a semantic root.
    Module {
        /// Root that owns the logical module.
        owner: Box<Self>,
        /// Complete logical module path in semantic order.
        path: Box<[String]>,
    },
    /// Compiler-provided declaration with its catalog identity and semantic category.
    CompilerKnownDeclaration {
        /// Stable compiler catalog key.
        key: String,
        /// Semantic category of the declaration.
        kind: DiagnosticInterfaceSymbolKind,
    },
    /// Source declaration with its owner, category, and stable declaration ordinal.
    SourceDeclaration {
        /// Immediate semantic owner.
        owner: Box<Self>,
        /// Semantic category of the declaration.
        kind: DiagnosticInterfaceSymbolKind,
        /// Stable declaration-discovery identity.
        declaration: u32,
    },
    /// Interface-addressable declaration with an owner-relative identity.
    Declaration {
        /// Immediate semantic owner.
        owner: Box<Self>,
        /// Semantic category of the declaration.
        kind: DiagnosticInterfaceSymbolKind,
        /// Owner-relative identity rule.
        identity: DiagnosticInterfaceDeclarationIdentity,
    },
    /// Declaration-surface symbol synthesized from another exact identity.
    Synthesized {
        /// Semantic identity from which the symbol is synthesized.
        owner: Box<Self>,
        /// Closed synthesized role and its required ordinal.
        identity: DiagnosticInterfaceSynthesizedIdentity,
    },
}

/// Exact imported identity-surface contract rejected during interface validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceIdentitySurfaceProblem {
    /// The identity surface contains no package root.
    Empty,
    /// The identity table is too large for compact interface IDs.
    SymbolCountOverflow,
    /// A record ID does not match its table position.
    NonCanonicalSymbolId {
        /// Required table-position ID.
        expected: u32,
        /// ID supplied by the record.
        actual: u32,
    },
    /// The first identity record is not a package root.
    MissingPackageRoot {
        /// Symbol category found at the root position.
        actual: DiagnosticInterfaceSymbolKind,
    },
    /// The package root names a semantic container.
    PackageRootHasContainer {
        /// Invalid container ID.
        container: u32,
    },
    /// A key belongs to a different package.
    PackageIdentityMismatch {
        /// Record containing the mismatched key.
        symbol: u32,
    },
    /// A record category disagrees with its structured key.
    SymbolKindMismatch {
        /// Mismatched record.
        symbol: u32,
        /// Category declared by the record.
        declared: DiagnosticInterfaceSymbolKind,
        /// Category encoded by the key.
        keyed: DiagnosticInterfaceSymbolKind,
    },
    /// Two records use the same external key.
    DuplicateExternalKey {
        /// First record using the key.
        first: u32,
        /// Later duplicate record.
        duplicate: u32,
    },
    /// A non-root record has no semantic container.
    MissingContainer {
        /// Uncontained record.
        symbol: u32,
    },
    /// A record names a missing, later, or cyclic container.
    InvalidContainer {
        /// Contained record.
        symbol: u32,
        /// Invalid container ID.
        container: u32,
    },
    /// A containment edge disagrees with the external key owner.
    ContainerKeyMismatch {
        /// Contained record.
        symbol: u32,
        /// Declared container ID.
        container: u32,
    },
    /// A second root appears in the package identity surface.
    UnexpectedRoot {
        /// Invalid root record.
        symbol: u32,
        /// Invalid root category.
        kind: DiagnosticInterfaceSymbolKind,
    },
}

impl DiagnosticInterfaceIdentitySurfaceProblem {
    /// Returns the stable machine key for this identity-surface failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::SymbolCountOverflow => "symbol_count_overflow",
            Self::NonCanonicalSymbolId { .. } => "non_canonical_symbol_id",
            Self::MissingPackageRoot { .. } => "missing_package_root",
            Self::PackageRootHasContainer { .. } => "package_root_has_container",
            Self::PackageIdentityMismatch { .. } => "package_identity_mismatch",
            Self::SymbolKindMismatch { .. } => "symbol_kind_mismatch",
            Self::DuplicateExternalKey { .. } => "duplicate_external_key",
            Self::MissingContainer { .. } => "missing_container",
            Self::InvalidContainer { .. } => "invalid_container",
            Self::ContainerKeyMismatch { .. } => "container_key_mismatch",
            Self::UnexpectedRoot { .. } => "unexpected_root",
        }
    }
}

/// Complete relationship record retained by interface validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticInterfaceRelationship {
    kind: DiagnosticInterfaceRelationshipKind,
    owner: u32,
    member: u32,
    ordinal: u32,
    position: crate::DiagnosticCallablePosition,
    allows_mutation: bool,
}

impl DiagnosticInterfaceRelationship {
    /// Creates one exact interface relationship record.
    pub const fn new(
        kind: DiagnosticInterfaceRelationshipKind,
        owner: u32,
        member: u32,
        ordinal: u32,
        position: crate::DiagnosticCallablePosition,
        allows_mutation: bool,
    ) -> Self {
        Self {
            kind,
            owner,
            member,
            ordinal,
            position,
            allows_mutation,
        }
    }

    /// Returns the relationship category.
    pub const fn kind(self) -> DiagnosticInterfaceRelationshipKind {
        self.kind
    }

    /// Returns the owner record ID.
    pub const fn owner(self) -> u32 {
        self.owner
    }

    /// Returns the member record ID.
    pub const fn member(self) -> u32 {
        self.member
    }

    /// Returns the owner-relative ordinal.
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }

    /// Returns the callable-position contract stored by the relationship.
    pub const fn position(self) -> crate::DiagnosticCallablePosition {
        self.position
    }

    /// Returns whether the related field permits mutation after initialization.
    pub const fn allows_mutation(self) -> bool {
        self.allows_mutation
    }
}

/// Locale-neutral exact reason an imported symbol graph could not be constructed.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceSymbolGraphProblem {
    DuplicateInterface(u32),
    DuplicatePackage(String),
    SymbolCapacityExceeded {
        actual: u64,
        maximum: u64,
    },
    DuplicateExternalIdentity(DiagnosticInterfaceSymbolIdentity),
    RelationshipSymbolOutOfBounds {
        interface: u32,
        symbol: u32,
    },
    InvalidRelationshipKinds {
        relationship: DiagnosticInterfaceRelationshipKind,
        owner: DiagnosticInterfaceSymbolKind,
        member: DiagnosticInterfaceSymbolKind,
    },
    RelationshipContainmentMismatch {
        owner: u32,
        member: u32,
    },
    NonCanonicalRelationshipOrdinal {
        relationship: DiagnosticInterfaceRelationshipKind,
        owner: u32,
        expected: u64,
        actual: u32,
    },
    MissingContainment {
        symbol: u32,
        kind: DiagnosticInterfaceSymbolKind,
    },
    DuplicateContainment {
        symbol: u32,
        kind: DiagnosticInterfaceSymbolKind,
    },
    LookupOwnerOutOfBounds {
        interface: u32,
        owner: u32,
    },
    InvalidLookupOwner {
        owner: u32,
        kind: DiagnosticInterfaceSymbolKind,
    },
    MissingLookupTarget(DiagnosticInterfaceSymbolIdentity),
    DuplicateLookupName {
        owner: u32,
        name: String,
    },
    UnsupportedSymbolKind(DiagnosticInterfaceSymbolKind),
    InvalidRecordRelationships {
        symbol: u32,
        kind: DiagnosticInterfaceSymbolKind,
    },
    /// A package-interface surface describes a non-library product.
    SurfaceNonLibraryProduct,
    /// A package-interface dependency table exceeds compact IDs.
    SurfaceDependencyCountOverflow,
    /// Two surface dependencies use the same package identity.
    SurfaceDuplicateDependencyPackage(String),
    /// Surface symbols do not use their required stable order.
    SurfaceNonCanonicalSymbolOrder {
        /// Previous symbol record.
        previous: u32,
        /// Out-of-order current symbol record.
        current: u32,
    },
    /// A decoded identity surface violates its exact contract.
    SurfaceIdentity(DiagnosticInterfaceIdentitySurfaceProblem),
    /// A relationship references a symbol outside the identity table.
    SurfaceRelationshipSymbolOutOfBounds(DiagnosticInterfaceRelationship),
    /// A relationship's semantic shape is invalid.
    SurfaceInvalidRelationship(DiagnosticInterfaceRelationship),
    /// Two relationships occupy the same owner-relative position.
    SurfaceDuplicateRelationshipPosition(DiagnosticInterfaceRelationship),
    /// An exported lookup owner is outside the identity table.
    SurfaceExportOwnerOutOfBounds(u32),
    /// A symbol that cannot own exported lookups does so.
    SurfaceInvalidExportOwner(u32),
    /// A local export target is outside the identity table.
    SurfaceExportTargetOutOfBounds(u32),
    /// A dependency target names a missing dependency slot.
    SurfaceDependencyOutOfBounds(u32),
    /// A dependency target key belongs to another package.
    SurfaceDependencyKeyPackageMismatch(u32),
    /// A direct export targets a declaration outside its owner.
    SurfaceInvalidDirectExportTarget(u32),
    /// Two export edges project the same name from one owner.
    SurfaceDuplicateExportName {
        /// Exporting package or module record.
        owner: u32,
        /// Duplicate projected name.
        name: String,
    },
}

/// Locale-neutral exact imported-symbol reference retained by a semantic diagnostic.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceSymbolReference {
    Local(u32),
    Dependency {
        dependency: u32,
        identity: DiagnosticInterfaceSymbolIdentity,
    },
    CompilerKnown(DiagnosticInterfaceSymbolIdentity),
}

/// Locale-neutral exact compiled semantic-content contract violation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSemanticContentProblem {
    ForeignId {
        expected: u64,
        actual: u64,
    },
    UnknownId {
        value_kind: DiagnosticSemanticValueKind,
    },
    CapacityExhausted {
        value_kind: DiagnosticSemanticValueKind,
    },
    GenericOwnerMismatch {
        expected_kind: DiagnosticInterfaceSymbolKind,
        expected: u32,
        actual_kind: DiagnosticInterfaceSymbolKind,
        actual: u32,
    },
    OpenSubstitution,
    /// A returned-dependency reference does not name an enclosing equation.
    InvalidDependencyVariable {
        /// Number of enclosing equation groups to skip.
        depth: u32,
        /// Equation ordinal within the selected group.
        ordinal: u32,
    },
}

/// Locale-neutral exact checked-template contract violation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCheckedTemplateProblem {
    CapacityExceeded,
    RecoveredTemplate,
    MissingInput(u32),
    DuplicateInput {
        first: u32,
        duplicate: u32,
    },
    InputTypeMismatch {
        node: u32,
        input: u32,
        expected_type: u32,
        actual_type: u32,
    },
    MissingNode(u32),
    ForwardNodeReference {
        node: u32,
        referenced: u32,
    },
    MissingTemporary(u32),
    UninitializedTemporary {
        node: u32,
        temporary: u32,
    },
    TemporaryInitializerTypeMismatch {
        initializer: u32,
        expected_type: u32,
        actual_type: u32,
    },
    TemporaryTypeMismatch {
        node: u32,
        temporary: u32,
        expected_type: u32,
        actual_type: u32,
    },
    ConversionTypeMismatch {
        node: u32,
        expected_type: u32,
        actual_type: u32,
    },
    ConditionalBranchTypeMismatch {
        node: u32,
        when_true_type: u32,
        when_false_type: u32,
    },
    ConditionalResultTypeMismatch {
        node: u32,
        expected_type: u32,
        actual_type: u32,
    },
    ShortCircuitOperandTypeMismatch {
        node: u32,
        left_type: u32,
        right_type: u32,
    },
    ShortCircuitResultTypeMismatch {
        node: u32,
        expected_type: u32,
        actual_type: u32,
    },
    ArrayElementTypeMismatch {
        node: u32,
        element: u32,
        expected_type: u32,
        actual_type: u32,
    },
}

/// Locale-neutral exact reason imported semantic content could not be represented.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceSemanticProblem {
    UnresolvedSymbol(DiagnosticInterfaceSymbolReference),
    InvalidSymbolKind(DiagnosticInterfaceSymbolReference),
    UnresolvedValueGraph,
    SemanticContent(DiagnosticSemanticContentProblem),
    InvalidTemplate(DiagnosticCheckedTemplateProblem),
    InvalidSupportEntity(u32),
}
