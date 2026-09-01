use super::{DiagnosticProductDataKind, DiagnosticProductQueryContext, DiagnosticProductValueKind};

/// Exact locale-neutral product-query contract failure retained for emission diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticProductQueryFailure {
    /// Required data is unavailable for an exact product context.
    Missing {
        /// The exact operation or identity requiring the data.
        context: DiagnosticProductQueryContext,
        /// The missing data category.
        data: DiagnosticProductDataKind,
    },
    /// Two values conflict for one exact product context and data category.
    Conflict {
        /// The exact operation or identity owning the values.
        context: DiagnosticProductQueryContext,
        /// The conflicting data category.
        data: DiagnosticProductDataKind,
    },
    /// A product value has an incompatible category.
    UnexpectedKind {
        /// The exact operation or identity receiving the value.
        context: DiagnosticProductQueryContext,
        /// The required value category.
        expected: DiagnosticProductValueKind,
        /// The actual value category.
        actual: DiagnosticProductValueKind,
    },
    /// A product collection has an incompatible count.
    CountMismatch {
        /// The exact operation or identity owning the collection.
        context: DiagnosticProductQueryContext,
        /// The counted data category.
        data: DiagnosticProductDataKind,
        /// The required count.
        expected: usize,
        /// The actual count.
        actual: usize,
    },
    /// A constant value category cannot participate in specialization identity.
    UnsupportedConstantValue {
        /// Exact constant-value identity.
        value: String,
        /// Locale-neutral retained value category.
        kind: String,
    },
    /// A generic substitution violated its ordered shape.
    GenericSubstitution {
        /// Exact substitution identity when already interned.
        substitution: Option<String>,
        /// Exact locale-neutral substitution-shape cause.
        cause: String,
    },
    /// A selected implementation witness describes another trait application.
    TraitApplicationMismatch {
        /// Exact implementation witness identity.
        witness: String,
        /// Required trait-application identity.
        expected: String,
        /// Actual trait-application identity.
        actual: String,
    },
    /// Two implementations map to one code-generation witness identity.
    ConflictingImplementationWitness {
        /// Exact structural code-generation witness identity.
        identity: String,
        /// First implementation-instance identity.
        existing: String,
        /// Conflicting implementation-instance identity.
        actual: String,
    },
    /// A semantic type retained data incompatible with a product operation.
    UnexpectedSemanticType {
        /// Exact semantic-type identity.
        ty: String,
        /// Required semantic-type category.
        expected: DiagnosticProductValueKind,
        /// Exact locale-neutral retained semantic-type data.
        actual: String,
    },
    /// A product coordination lock was poisoned.
    SynchronizationPoisoned {
        /// Exact locale-neutral coordination component.
        component: String,
    },
    /// A static lifecycle dependency count overflowed.
    StaticDependencyOverflow {
        /// Exact structural static-instance identity.
        static_instance: String,
    },
    /// A static lifecycle dependency count underflowed.
    StaticDependencyUnderflow {
        /// Exact structural static-instance identity.
        static_instance: String,
    },
    /// Static lifecycle dependencies contain a cycle.
    StaticLifecycleCycle {
        /// Exact structural identities left in the cycle.
        instances: Box<[String]>,
    },
    /// Two concrete realizations use one code-generation identity.
    ConflictingConcreteInstance {
        /// Exact structural code-generation instance identity.
        key: String,
    },
    /// Two static relocations use one constant-value leaf.
    ConflictingStaticRelocation {
        /// Exact constant-value identity.
        value: String,
    },
    /// A concrete instance retained another callable definition.
    CallableDefinitionMismatch {
        /// Exact structural code-generation instance identity.
        instance: String,
        /// Required callable-definition identity.
        expected: u32,
        /// Actual callable-definition identity.
        actual: u32,
    },
    /// A selected trait context belongs to another trait definition.
    TraitDefinitionMismatch {
        /// Exact operation or identity owning the relationship.
        context: DiagnosticProductQueryContext,
        /// Required trait-definition identity.
        expected: u32,
        /// Actual trait-definition identity.
        actual: u32,
    },
    /// A source cannot be assigned a code-generation file identity.
    InvalidCodegenSourceFile {
        /// Loaded source identity, when available.
        source: Option<u32>,
        /// Exact rejected source path.
        path: String,
    },
    /// A source line index exceeded the compact text-offset range.
    SourceIndex {
        /// Exact loaded source identity.
        source: u32,
        /// Exact locale-neutral overflow cause.
        cause: String,
    },
    /// External-symbol identity construction failed for one semantic symbol.
    ExternalSymbolIdentity {
        /// Exact semantic symbol identity.
        symbol: String,
        /// Exact locale-neutral package-interface cause.
        cause: String,
    },
    /// A compiler-known declaration-key literal is invalid.
    InvalidCompilerKnownDeclarationKey {
        /// Exact rejected declaration key.
        key: String,
    },
    /// A recognized standard-library declaration-key literal is invalid.
    InvalidRecognizedStandardLibraryDeclarationKey {
        /// Exact rejected declaration key.
        key: String,
    },
    /// A package-identity literal is invalid.
    InvalidPackageIdentity {
        /// Exact rejected package identity.
        identity: String,
    },
    /// An executable entry produced an unsupported result shape.
    UnexpectedEntryResult {
        /// Exact locale-neutral entry-result shape.
        actual: String,
    },
    /// A semantic type maps to another compiler-known representation.
    CompilerKnownRepresentationMismatch {
        /// Exact semantic-type identity.
        ty: String,
        /// Required representation role.
        expected: String,
        /// Actual representation role, when compiler-known.
        actual: Option<String>,
    },
    /// An imported static reference resolved to another native-boundary kind.
    NativeBoundaryKindMismatch {
        /// Exact structural static-reference identity.
        reference: String,
        /// Actual native-boundary kind.
        actual: String,
    },
    /// A semantic symbol has an incompatible category.
    UnexpectedSymbolKind {
        /// Exact semantic-symbol identity.
        symbol: String,
        /// Required symbol category.
        expected: String,
        /// Actual symbol category.
        actual: String,
    },
    /// A witness key does not identify an implementation declaration.
    ImplementationSymbolKeyExpected {
        /// Exact structural symbol key.
        key: String,
        /// Actual symbol category.
        actual: String,
    },
    /// A target contract does not support an exact runtime role.
    UnsupportedRuntimeRole {
        /// Exact runtime ABI role.
        role: String,
    },
    /// A generated helper encountered an incompatible MIR operation.
    InvalidHelperOperation {
        /// Exact owning product-query context.
        context: DiagnosticProductQueryContext,
        /// Exact structural helper identity.
        helper: String,
        /// Exact retained MIR operation kind.
        operation: String,
    },
    /// A generated helper encountered an incompatible call target.
    InvalidHelperCallTarget {
        /// Exact owning product-query context.
        context: DiagnosticProductQueryContext,
        /// Exact structural helper identity.
        helper: String,
        /// Exact retained call target.
        target: String,
    },
    /// A generated lifecycle instance and helper disagree on lifecycle role.
    LifecycleRoleMismatch {
        /// Exact structural code-generation instance identity.
        instance: String,
        /// Required lifecycle role, when present.
        expected: Option<String>,
        /// Actual lifecycle role, when present.
        actual: Option<String>,
    },
    /// A built-in proof did not establish an implementation requirement.
    BuiltInProofMismatch {
        /// Exact structural implementation requirement.
        requirement: String,
        /// Exact proof outcome, when available.
        actual: Option<String>,
    },
    /// Implementation selection did not produce a required witness.
    ImplementationSelectionMismatch {
        /// Exact structural implementation requirement.
        requirement: String,
        /// Exact retained selection result.
        actual: String,
    },
    /// A runtime-default provider belongs to an unsupported subject.
    UnsupportedRuntimeDefaultSubject {
        /// Exact provider symbol identity.
        provider: String,
        /// Exact unsupported subject identity.
        subject: String,
    },
    /// A semantic symbol cannot serve as a callable definition.
    InvalidCallableDefinitionSymbol {
        /// Exact semantic-symbol identity.
        symbol: String,
        /// Actual symbol category.
        actual: String,
    },
    /// A lifecycle role cannot be emitted as a lifecycle operation.
    UnsupportedLifecycleRole {
        /// Exact lifecycle role.
        role: String,
    },
    /// A loaded source snapshot differs from generated MIR.
    SourceSnapshotMismatch {
        /// Exact source identity.
        source: u32,
        /// Required source version.
        expected: String,
        /// Loaded source version, when available.
        actual: Option<String>,
    },
    /// Test discovery was requested for another product contract.
    TestProductMismatch {
        /// Exact requested product identity.
        requested: String,
        /// Compilation-owned package identity.
        compilation_package: String,
        /// Compilation-owned product category.
        compilation_kind: String,
    },
    /// Canonical test-catalog construction rejected exact metadata.
    TestCatalog {
        /// Exact selected product identity.
        product: String,
        /// Exact rejected test identity.
        identity: String,
        /// Exact locale-neutral catalog contract failure.
        cause: String,
    },
    /// A stable test error-type identity could not be constructed.
    InvalidTestErrorTypeIdentity {
        /// Exact structural type digest.
        digest: crate::DiagnosticArtifactDigest,
    },
    /// A discovered module path is not representable as a symbol path.
    InvalidModulePath {
        /// Exact declaration identity.
        declaration: u32,
        /// Exact rejected path segments.
        segments: Box<[String]>,
    },
    /// An entry result type has no supported lifecycle representation.
    UnsupportedEntryResultType {
        /// Exact semantic-type identity.
        ty: String,
        /// Compiler-known representation, when present.
        actual: Option<String>,
    },
    /// A code-generation unit could not form an exact backend request.
    InvalidCodegenRequest {
        /// Exact code-generation unit identity.
        unit: crate::DiagnosticArtifactDigest,
        /// Exact locale-neutral request contract failure.
        cause: String,
    },
    /// The backend registry rejected an exact unit request.
    CodegenBackendSelection {
        /// Exact code-generation unit identity.
        unit: crate::DiagnosticArtifactDigest,
        /// Exact locale-neutral backend-selection failure.
        cause: String,
    },
}

impl DiagnosticProductQueryFailure {
    /// Returns this failure category's stable machine-readable name.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Missing { .. } => "product_query_missing",
            Self::Conflict { .. } => "product_query_conflict",
            Self::UnexpectedKind { .. } => "product_query_unexpected_kind",
            Self::CountMismatch { .. } => "product_query_count_mismatch",
            Self::UnsupportedConstantValue { .. } => "product_query_unsupported_constant_value",
            Self::GenericSubstitution { .. } => "product_query_generic_substitution",
            Self::TraitApplicationMismatch { .. } => "product_query_trait_application_mismatch",
            Self::ConflictingImplementationWitness { .. } => {
                "product_query_conflicting_implementation_witness"
            }
            Self::UnexpectedSemanticType { .. } => "product_query_unexpected_semantic_type",
            Self::SynchronizationPoisoned { .. } => "product_query_synchronization_poisoned",
            Self::StaticDependencyOverflow { .. } => "product_query_static_dependency_overflow",
            Self::StaticDependencyUnderflow { .. } => "product_query_static_dependency_underflow",
            Self::StaticLifecycleCycle { .. } => "product_query_static_lifecycle_cycle",
            Self::ConflictingConcreteInstance { .. } => {
                "product_query_conflicting_concrete_instance"
            }
            Self::ConflictingStaticRelocation { .. } => {
                "product_query_conflicting_static_relocation"
            }
            Self::CallableDefinitionMismatch { .. } => "product_query_callable_definition_mismatch",
            Self::TraitDefinitionMismatch { .. } => "product_query_trait_definition_mismatch",
            Self::InvalidCodegenSourceFile { .. } => "product_query_invalid_codegen_source_file",
            Self::SourceIndex { .. } => "product_query_source_index",
            Self::ExternalSymbolIdentity { .. } => "product_query_external_symbol_identity",
            Self::InvalidCompilerKnownDeclarationKey { .. } => {
                "product_query_invalid_compiler_known_declaration_key"
            }
            Self::InvalidRecognizedStandardLibraryDeclarationKey { .. } => {
                "product_query_invalid_recognized_standard_library_declaration_key"
            }
            Self::InvalidPackageIdentity { .. } => "product_query_invalid_package_identity",
            Self::UnexpectedEntryResult { .. } => "product_query_unexpected_entry_result",
            Self::CompilerKnownRepresentationMismatch { .. } => {
                "product_query_compiler_known_representation_mismatch"
            }
            Self::NativeBoundaryKindMismatch { .. } => {
                "product_query_native_boundary_kind_mismatch"
            }
            Self::UnexpectedSymbolKind { .. } => "product_query_unexpected_symbol_kind",
            Self::ImplementationSymbolKeyExpected { .. } => {
                "product_query_implementation_symbol_key_expected"
            }
            Self::UnsupportedRuntimeRole { .. } => "product_query_unsupported_runtime_role",
            Self::InvalidHelperOperation { .. } => "product_query_invalid_helper_operation",
            Self::InvalidHelperCallTarget { .. } => "product_query_invalid_helper_call_target",
            Self::LifecycleRoleMismatch { .. } => "product_query_lifecycle_role_mismatch",
            Self::BuiltInProofMismatch { .. } => "product_query_built_in_proof_mismatch",
            Self::ImplementationSelectionMismatch { .. } => {
                "product_query_implementation_selection_mismatch"
            }
            Self::UnsupportedRuntimeDefaultSubject { .. } => {
                "product_query_unsupported_runtime_default_subject"
            }
            Self::InvalidCallableDefinitionSymbol { .. } => {
                "product_query_invalid_callable_definition_symbol"
            }
            Self::UnsupportedLifecycleRole { .. } => "product_query_unsupported_lifecycle_role",
            Self::SourceSnapshotMismatch { .. } => "product_query_source_snapshot_mismatch",
            Self::TestProductMismatch { .. } => "product_query_test_product_mismatch",
            Self::TestCatalog { .. } => "product_query_test_catalog",
            Self::InvalidTestErrorTypeIdentity { .. } => {
                "product_query_invalid_test_error_type_identity"
            }
            Self::InvalidModulePath { .. } => "product_query_invalid_module_path",
            Self::UnsupportedEntryResultType { .. } => {
                "product_query_unsupported_entry_result_type"
            }
            Self::InvalidCodegenRequest { .. } => "product_query_invalid_codegen_request",
            Self::CodegenBackendSelection { .. } => "product_query_codegen_backend_selection",
        }
    }
}
