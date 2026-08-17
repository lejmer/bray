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
        expected: u32,
        actual: u32,
    },
    OpenSubstitution,
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
