use std::path::PathBuf;

use super::checker::DiagnosticCheckerFailure;
use crate::{
    DiagnosticArtifactDigest, DiagnosticArtifactKind, DiagnosticArtifactRequirement,
    DiagnosticAssemblySyntaxKind, DiagnosticDebugInformationMode, DiagnosticDebugOutputMode,
    DiagnosticFactRuntimeFailure, DiagnosticIoErrorKind, DiagnosticLinkInputKind,
    DiagnosticLinkedArtifactKind, DiagnosticLinkedProductKind, DiagnosticOutputSink,
    DiagnosticProductKind, DiagnosticProductQueryFailure,
};

/// Locale-neutral identity of one artifact in an emission operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticEmissionArtifact {
    kind: DiagnosticArtifactKind,
    ordinal: u32,
}

impl DiagnosticEmissionArtifact {
    pub const fn new(kind: DiagnosticArtifactKind, ordinal: u32) -> Self {
        Self { kind, ordinal }
    }

    pub const fn kind(self) -> DiagnosticArtifactKind {
        self.kind
    }

    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }
}

/// Locale-neutral exact reason a product emission operation could not complete.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticEmissionFailure {
    InvalidRequest,
    Planning(DiagnosticEmissionPlanningFailure),
    PackageInterface(DiagnosticPackageInterfaceFailure),
    Codegen(DiagnosticEmissionCodegenFailure),
    Staging(DiagnosticEmissionStagingFailure),
    LinkPlan(DiagnosticEmissionLinkPlanFailure),
    Evaluation(DiagnosticEmissionEvaluationFailure),
    /// Test-catalog encoding failed before the output could be published.
    TestCatalog(DiagnosticTestCatalogFailure),
    MissingContribution(DiagnosticEmissionArtifact),
    InvalidContribution(DiagnosticEmissionArtifact),
    Publication(DiagnosticEmissionArtifact),
    Linking,
    // rust-style: broad-failure
    IncompleteProduct,
}

/// Exact test-catalog protocol failure observed before publication.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticTestCatalogFailure {
    /// The protocol encoder encountered an unexpected I/O path.
    Io,
    /// The discovered catalog violated the protocol's canonical shape.
    Malformed,
    /// The catalog requested a protocol version this build cannot encode.
    UnsupportedVersion(u32),
    /// The catalog exceeded a bounded protocol resource.
    ResourceLimit,
}

impl DiagnosticTestCatalogFailure {
    /// Returns the stable machine key for this failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Io => "test_catalog_io",
            Self::Malformed => "test_catalog_malformed",
            Self::UnsupportedVersion(_) => "test_catalog_unsupported_version",
            Self::ResourceLimit => "test_catalog_resource_limit",
        }
    }
}

/// Exact planning contract that rejected an emission request.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticEmissionPlanningFailure {
    Incomplete,
    ProductArtifactMismatch {
        product: DiagnosticProductKind,
        artifact: DiagnosticArtifactKind,
    },
    MissingPackageInterfaceArtifact,
    MissingTestCatalogArtifact,
    UnexpectedPackageInterfaceArtifact,
    PackageInterfaceProductMismatch {
        expected: String,
        actual: String,
    },
    MissingLinkedProduct,
    MultipleLinkedProducts,
    LinkedCompanionRequirementMismatch,
    MissingBackend,
    MissingCodegenUnits,
    UnsupportedBackendTarget(String),
    UnsupportedBackendArtifact(DiagnosticArtifactKind),
    UnsupportedDebugInformation(DiagnosticDebugInformationMode),
    UnsupportedDebugOutput(DiagnosticDebugOutputMode),
    UnsupportedAssemblySyntax(DiagnosticAssemblySyntaxKind),
    InvalidDebugOutput {
        information: DiagnosticDebugInformationMode,
        output: DiagnosticDebugOutputMode,
    },
    MissingRequiredDebugCompanion,
    UnexpectedDebugCompanion,
    MissingLinkableArtifact,
    MissingSerializationArtifact(DiagnosticArtifactKind),
    MissingOutputName(DiagnosticArtifactKind),
    InvalidProductName,
    InvalidGeneratedFileName(DiagnosticArtifactKind),
    MultipleArtifactsForSingleSink,
    MissingExplicitFileName,
    InvalidExplicitFileName,
    ExplicitOutputSuffixMismatch(DiagnosticArtifactKind),
    ManagedProductDestinationRequired,
    ArtifactOrdinalOverflow(DiagnosticArtifactKind),
    OutputCollision(DiagnosticOutputSink),
    BackendRequestEmpty,
    BackendRequestForeignUnit(DiagnosticEmissionArtifact),
    BackendRequestDuplicateIdentity(DiagnosticEmissionArtifact),
    BackendRequestMissingLinkableArtifact {
        kind: DiagnosticArtifactKind,
        requirement: DiagnosticArtifactRequirement,
    },
    BackendRequestMissingRequiredDebugCompanion,
    BackendRequestUnexpectedDebugCompanion,
    BackendRequestUnexpectedAssemblySyntax,
    /// Specialized bitcode semantics were selected without a bitcode contribution.
    BackendRequestUnexpectedBitcodeSemantics,
    InconsistentPlan,
}

/// Exact package-interface or implementation contract that rejected publication.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticPackageInterfaceFailure {
    Unavailable,
    // rust-style: broad-failure
    InvalidCompilation,
    InvalidCompilationCause {
        reason: &'static str,
        context: Box<[crate::DiagnosticFailureField]>,
    },
    SemanticValueStoreCreate,
    SemanticValue(DiagnosticSemanticValueFailure),
    RecoveredPublicSymbol(String),
    ConstantCallableEvaluation {
        declaration: crate::DiagnosticInterfaceSymbolIdentity,
        cause: DiagnosticEmissionEvaluationFailure,
    },
    ExecutableTemplateEvaluation {
        declaration: crate::DiagnosticInterfaceSymbolIdentity,
        cause: DiagnosticEmissionEvaluationFailure,
    },
    IncompletePublicDeclaration(String),
    DuplicateSymbol(String),
    MissingSymbol(String),
    SymbolCountOverflow,
    SymbolGraph(crate::DiagnosticInterfaceSymbolGraphProblem),
    MissingDeclarationData(crate::DiagnosticInterfaceSymbolIdentity),
    LostDeclarationReference {
        declaration: Option<crate::DiagnosticInterfaceSymbolIdentity>,
        table: String,
        reference: u32,
    },
    DuplicateDeclarationIdentity {
        first: crate::DiagnosticInterfaceSymbolIdentity,
        second: crate::DiagnosticInterfaceSymbolIdentity,
        identity: crate::DiagnosticInterfaceSymbolIdentity,
    },
    RecursiveDeclarationData {
        declaration: Option<crate::DiagnosticInterfaceSymbolIdentity>,
        table: String,
        reference: u32,
    },
    DeclarationDiscoveryFailure {
        cause: DiagnosticEmissionEvaluationFailure,
        cycle: Box<[String]>,
    },
    MissingPackageReference {
        table: String,
        reference: u32,
    },
    MisassignedPackageRecord(String),
    ConflictingDeclarationRecord {
        kind: String,
        owner: crate::DiagnosticInterfaceSymbolReference,
    },
    RecursiveDeclarationReference(String),
    SemanticTableOverflow {
        table: String,
        maximum: u32,
    },
    DuplicateConstantCallableBody(u32),
    DuplicateExecutableTemplate(u32),
    InvalidExecutableTemplateFamily(u32),
    DuplicateNativeBoundary(u32),
    ImplementationDuplicateCallableBody(u32),
    ImplementationDuplicateExecutableTemplate(u32),
    ImplementationInvalidExecutableTemplateFamily(u32),
    ImplementationDuplicateNativeBoundary(u32),
    ImplementationDuplicateSpecialization,
    ImplementationSpecializationIdentityMismatch,
    ImplementationInvalidExecutableOwner(u32),
    ImplementationInvalidNativeBoundaryOwner(u32),
    ImplementationInvalidCallableOwner(u32),
    ImplementationContentTooLarge,
}

/// Exact code-generation or contribution-merge contract that failed.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticEmissionCodegenFailure {
    Incomplete,
    DuplicateUnit(DiagnosticArtifactDigest),
    DuplicateMappings(DiagnosticArtifactDigest),
    MissingUnit(DiagnosticArtifactDigest),
    MissingMappings(DiagnosticArtifactDigest),
    Request(DiagnosticArtifactDigest),
    Generation(DiagnosticArtifactDigest),
    MergeDuplicateUnit(DiagnosticArtifactDigest),
    MergeMissingUnit(DiagnosticArtifactDigest),
    MergeUnrequestedUnit(DiagnosticArtifactDigest),
    MergeBackendMismatch(DiagnosticArtifactDigest),
    MergeCapabilityMismatch(DiagnosticArtifactDigest),
    MergeTargetMismatch(DiagnosticArtifactDigest),
    MergeMissingArtifact(DiagnosticEmissionArtifact),
    MergeUnrequestedArtifact(DiagnosticEmissionArtifact),
    MergeArtifactKindMismatch(DiagnosticEmissionArtifact),
    MergeReadFailed {
        artifact: DiagnosticEmissionArtifact,
        error: DiagnosticIoErrorKind,
    },
    MergeLengthMismatch {
        artifact: DiagnosticEmissionArtifact,
        expected: u64,
        actual: u64,
    },
    MergeDigestMismatch {
        artifact: DiagnosticEmissionArtifact,
        expected: DiagnosticArtifactDigest,
        actual: DiagnosticArtifactDigest,
    },
    MergeInvalidContent(DiagnosticEmissionArtifact),
}

/// Exact staging contract that failed before linking.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticEmissionStagingFailure {
    Incomplete,
    DuplicateContribution(DiagnosticEmissionArtifact),
    MissingContribution(DiagnosticEmissionArtifact),
    InvalidContribution(DiagnosticEmissionArtifact),
    UnexpectedContribution(DiagnosticEmissionArtifact),
    UnsupportedOutput(DiagnosticEmissionArtifact),
    InvalidPath(DiagnosticEmissionArtifact),
}

/// Exact native link-plan contract that rejected the staged product.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticEmissionLinkPlanFailure {
    Incomplete,
    MissingLinkedProduct,
    MissingLinker,
    UnexpectedLinker,
    DuplicateStagedArtifact(DiagnosticEmissionArtifact),
    MissingStagedArtifact(DiagnosticEmissionArtifact),
    UnexpectedStagedArtifact(DiagnosticEmissionArtifact),
    UnsupportedStagedArtifact(DiagnosticEmissionArtifact),
    DuplicateOutputStaging(DiagnosticEmissionArtifact),
    MissingOutputStaging(DiagnosticEmissionArtifact),
    UnexpectedOutputStaging(DiagnosticEmissionArtifact),
    OutputKindMismatch {
        artifact: DiagnosticEmissionArtifact,
        actual: DiagnosticLinkedArtifactKind,
    },
    InvalidStartupInputKind(DiagnosticLinkInputKind),
    InvalidNativeInputKind(DiagnosticLinkInputKind),
    InvalidTerminationInputKind(DiagnosticLinkInputKind),
    InputOrdinalOverflow,
    OutputOrdinalOverflow,
    InputEmptyPath,
    InputSourceKindMismatch,
    WholeArchiveRequiresArchive,
    OutputEmptyPath,
    MissingInputs,
    DuplicateInput(u32),
    DuplicateSearchPath,
    MissingRuntimeComponent,
    UnexpectedRuntimeComponent,
    RuntimeArtifactMismatch,
    MissingPrimaryOutput,
    MultiplePrimaryOutputs,
    OptionalPrimaryOutput,
    DuplicateOutput(u32),
    OutputPathCollision {
        first: u32,
        second: u32,
    },
    MissingStartupMode,
    UnexpectedStartupMode,
    MissingStartupInput,
    UnexpectedStartupInput(u32),
    MissingDebugCompanion,
    UnexpectedDebugCompanion,
    IncompatibleOutputKind {
        product: DiagnosticLinkedProductKind,
        artifact: DiagnosticLinkedArtifactKind,
    },
    InvalidOutputPath(PathBuf),
}

/// Exact compiler-evaluation failure observed during emission.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticEmissionEvaluationFailure {
    Cancelled,
    Cycle(DiagnosticEvaluationFailureDetail),
    Runtime(DiagnosticFactRuntimeFailure),
    SemanticValueStoreCreate,
    SemanticValue(DiagnosticSemanticValueFailure),
    Binding(DiagnosticBindingFailure),
    MirCapacity,
    ConstantCallableBodyUnavailable,
    ConstantCallableRootUnavailable,
    AtomicRepresentationTypeUnavailable,
    AtomicRepresentationArgumentsUnavailable,
    AtomicInitializerArgumentUnavailable,
    AtomicInitializerResultUnavailable,
    UninitInitializerResultUnavailable,
    ImportedExecutableTemplateMismatch,
    /// A semantic query failed with an exact compiler-owned category.
    SemanticQuery(DiagnosticSemanticQueryFailure),
    /// Product specialization or realization violated an exact retained contract.
    Product(DiagnosticProductQueryFailure),
    /// Foreign-boundary construction violated an exact retained contract.
    Foreign(crate::DiagnosticForeignQueryFailure),
    Checker(DiagnosticCheckerFailure),
}

/// Exact machine-readable detail retained for an evaluation-owned failure.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticEvaluationFailureDetail {
    reason: &'static str,
    context: Box<[crate::DiagnosticFailureField]>,
}

impl DiagnosticEvaluationFailureDetail {
    /// Creates one exact evaluation failure and its typed machine context.
    pub fn new(
        reason: &'static str,
        context: impl Into<Box<[crate::DiagnosticFailureField]>>,
    ) -> Self {
        Self {
            reason,
            context: context.into(),
        }
    }

    /// Returns the stable machine key for the exact failure.
    pub const fn reason(&self) -> &'static str {
        self.reason
    }

    /// Returns the typed context retained from the leaf failure.
    pub const fn context(&self) -> &[crate::DiagnosticFailureField] {
        &self.context
    }
}

/// Exact semantic-query failure retained for product diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticSemanticQueryFailure {
    category: &'static str,
    reason: &'static str,
    context: Box<[crate::DiagnosticFailureField]>,
}

impl DiagnosticSemanticQueryFailure {
    /// Creates one semantic-query failure with stable category, reason, and typed leaf context.
    pub fn new(
        category: &'static str,
        reason: &'static str,
        context: impl Into<Box<[crate::DiagnosticFailureField]>>,
    ) -> Self {
        Self {
            category,
            reason,
            context: context.into(),
        }
    }

    /// Returns the stable semantic-query domain category.
    pub const fn category(&self) -> &'static str {
        self.category
    }

    /// Returns the stable machine-readable leaf reason.
    pub const fn reason(&self) -> &'static str {
        self.reason
    }

    /// Returns the ordered locale-neutral fields retained from the leaf failure.
    pub const fn context(&self) -> &[crate::DiagnosticFailureField] {
        &self.context
    }

    /// Returns this failure's stable machine-readable name.
    pub const fn as_str(&self) -> &'static str {
        self.reason
    }
}

/// Exact compiler-owned failure observed while binding one source-level program element.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticBindingFailure {
    reason: &'static str,
    context: Box<[crate::DiagnosticFailureField]>,
    semantic_value: Option<DiagnosticSemanticValueFailure>,
}

impl DiagnosticBindingFailure {
    /// Creates one binding failure with its stable reason and typed leaf context.
    pub fn new(
        reason: &'static str,
        context: impl Into<Box<[crate::DiagnosticFailureField]>>,
    ) -> Self {
        Self {
            reason,
            context: context.into(),
            semantic_value: None,
        }
    }

    /// Creates a binding failure backed by an exact canonical semantic-value failure.
    pub fn semantic_value(failure: DiagnosticSemanticValueFailure) -> Self {
        Self {
            reason: failure.as_str(),
            context: Box::new([]),
            semantic_value: Some(failure),
        }
    }

    /// Returns the ordered locale-neutral fields retained from the binding failure.
    pub const fn context(&self) -> &[crate::DiagnosticFailureField] {
        &self.context
    }

    /// Returns the canonical semantic-value cause when it owns this binding failure.
    pub const fn semantic_value_failure(&self) -> Option<DiagnosticSemanticValueFailure> {
        self.semantic_value
    }
}

/// Exact canonical-value failure that prevented semantic binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSemanticValueFailure {
    CapacityExhausted { kind: &'static str },
}

impl DiagnosticEmissionFailure {
    pub const fn category(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "request",
            Self::Planning(_) => "planning",
            Self::PackageInterface(_) => "package_interface",
            Self::Codegen(_) => "codegen",
            Self::Staging(_) => "staging",
            Self::LinkPlan(_) => "link_plan",
            Self::Evaluation(_) => "evaluation",
            Self::TestCatalog(_) => "test_catalog",
            Self::MissingContribution(_) => "missing_contribution",
            Self::InvalidContribution(_) => "invalid_contribution",
            Self::Publication(_) => "publication",
            Self::Linking => "linking",
            Self::IncompleteProduct => "incomplete_product",
        }
    }

    pub const fn reason(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Planning(failure) => failure.as_str(),
            Self::PackageInterface(failure) => failure.as_str(),
            Self::Codegen(failure) => failure.as_str(),
            Self::Staging(failure) => failure.as_str(),
            Self::LinkPlan(failure) => failure.as_str(),
            Self::Evaluation(failure) => failure.as_str(),
            Self::TestCatalog(failure) => failure.as_str(),
            Self::MissingContribution(_) => "missing_contribution",
            Self::InvalidContribution(_) => "invalid_contribution",
            Self::Publication(_) => "publication",
            Self::Linking => "linking",
            Self::IncompleteProduct => "incomplete_product",
        }
    }
}

impl DiagnosticEmissionPlanningFailure {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Incomplete => "incomplete",
            Self::ProductArtifactMismatch { .. } => "product_artifact_mismatch",
            Self::MissingPackageInterfaceArtifact => "missing_package_interface_artifact",
            Self::MissingTestCatalogArtifact => "missing_test_catalog_artifact",
            Self::UnexpectedPackageInterfaceArtifact => "unexpected_package_interface_artifact",
            Self::PackageInterfaceProductMismatch { .. } => "package_interface_product_mismatch",
            Self::MissingLinkedProduct => "missing_linked_product",
            Self::MultipleLinkedProducts => "multiple_linked_products",
            Self::LinkedCompanionRequirementMismatch => "linked_companion_requirement_mismatch",
            Self::MissingBackend => "missing_backend",
            Self::MissingCodegenUnits => "missing_codegen_units",
            Self::UnsupportedBackendTarget(_) => "unsupported_backend_target",
            Self::UnsupportedBackendArtifact(_) => "unsupported_backend_artifact",
            Self::UnsupportedDebugInformation(_) => "unsupported_debug_information",
            Self::UnsupportedDebugOutput(_) => "unsupported_debug_output",
            Self::UnsupportedAssemblySyntax(_) => "unsupported_assembly_syntax",
            Self::InvalidDebugOutput { .. } => "invalid_debug_output",
            Self::MissingRequiredDebugCompanion => "missing_required_debug_companion",
            Self::UnexpectedDebugCompanion => "unexpected_debug_companion",
            Self::MissingLinkableArtifact => "missing_linkable_artifact",
            Self::MissingSerializationArtifact(_) => "missing_serialization_artifact",
            Self::MissingOutputName(_) => "missing_output_name",
            Self::InvalidProductName => "invalid_product_name",
            Self::InvalidGeneratedFileName(_) => "invalid_generated_file_name",
            Self::MultipleArtifactsForSingleSink => "multiple_artifacts_for_single_sink",
            Self::MissingExplicitFileName => "missing_explicit_file_name",
            Self::InvalidExplicitFileName => "invalid_explicit_file_name",
            Self::ExplicitOutputSuffixMismatch(_) => "explicit_output_suffix_mismatch",
            Self::ManagedProductDestinationRequired => "managed_product_destination_required",
            Self::ArtifactOrdinalOverflow(_) => "artifact_ordinal_overflow",
            Self::OutputCollision(_) => "output_collision",
            Self::BackendRequestEmpty => "backend_request_empty",
            Self::BackendRequestForeignUnit(_) => "backend_request_foreign_unit",
            Self::BackendRequestDuplicateIdentity(_) => "backend_request_duplicate_identity",
            Self::BackendRequestMissingLinkableArtifact { .. } => {
                "backend_request_missing_linkable_artifact"
            }
            Self::BackendRequestMissingRequiredDebugCompanion => {
                "backend_request_missing_required_debug_companion"
            }
            Self::BackendRequestUnexpectedDebugCompanion => {
                "backend_request_unexpected_debug_companion"
            }
            Self::BackendRequestUnexpectedAssemblySyntax => {
                "backend_request_unexpected_assembly_syntax"
            }
            Self::BackendRequestUnexpectedBitcodeSemantics => {
                "backend_request_unexpected_bitcode_semantics"
            }
            Self::InconsistentPlan => "inconsistent_plan",
        }
    }
}

impl DiagnosticPackageInterfaceFailure {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::InvalidCompilation => "invalid_compilation",
            Self::InvalidCompilationCause { reason, .. } => reason,
            Self::SemanticValueStoreCreate => "semantic_value_store_create",
            Self::SemanticValue(_) => "semantic_value",
            Self::RecoveredPublicSymbol(_) => "recovered_public_symbol",
            Self::ConstantCallableEvaluation { .. } => "constant_callable_evaluation",
            Self::ExecutableTemplateEvaluation { .. } => "executable_template_evaluation",
            Self::IncompletePublicDeclaration(_) => "incomplete_public_declaration",
            Self::DuplicateSymbol(_) => "duplicate_symbol",
            Self::MissingSymbol(_) => "missing_symbol",
            Self::SymbolCountOverflow => "symbol_count_overflow",
            Self::SymbolGraph(_) => "symbol_graph",
            Self::MissingDeclarationData(_) => "missing_declaration_data",
            Self::LostDeclarationReference { .. } => "lost_declaration_reference",
            Self::DuplicateDeclarationIdentity { .. } => "duplicate_declaration_identity",
            Self::RecursiveDeclarationData { .. } => "recursive_declaration_data",
            Self::DeclarationDiscoveryFailure { .. } => "declaration_discovery_failure",
            Self::MissingPackageReference { .. } => "missing_package_reference",
            Self::MisassignedPackageRecord(_) => "misassigned_package_record",
            Self::ConflictingDeclarationRecord { .. } => "conflicting_declaration_record",
            Self::RecursiveDeclarationReference(_) => "recursive_declaration_reference",
            Self::SemanticTableOverflow { .. } => "semantic_table_overflow",
            Self::DuplicateConstantCallableBody(_) => "duplicate_constant_callable_body",
            Self::DuplicateExecutableTemplate(_) => "duplicate_executable_template",
            Self::InvalidExecutableTemplateFamily(_) => "invalid_executable_template_family",
            Self::DuplicateNativeBoundary(_) => "duplicate_native_boundary",
            Self::ImplementationDuplicateCallableBody(_) => {
                "implementation_duplicate_callable_body"
            }
            Self::ImplementationDuplicateExecutableTemplate(_) => {
                "implementation_duplicate_executable_template"
            }
            Self::ImplementationInvalidExecutableTemplateFamily(_) => {
                "implementation_invalid_executable_template_family"
            }
            Self::ImplementationDuplicateNativeBoundary(_) => {
                "implementation_duplicate_native_boundary"
            }
            Self::ImplementationDuplicateSpecialization => {
                "implementation_duplicate_specialization"
            }
            Self::ImplementationSpecializationIdentityMismatch => {
                "implementation_specialization_identity_mismatch"
            }
            Self::ImplementationInvalidExecutableOwner(_) => {
                "implementation_invalid_executable_owner"
            }
            Self::ImplementationInvalidNativeBoundaryOwner(_) => {
                "implementation_invalid_native_boundary_owner"
            }
            Self::ImplementationInvalidCallableOwner(_) => "implementation_invalid_callable_owner",
            Self::ImplementationContentTooLarge => "implementation_content_too_large",
        }
    }
}

impl DiagnosticEmissionCodegenFailure {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Incomplete => "incomplete",
            Self::DuplicateUnit(_) => "duplicate_unit",
            Self::DuplicateMappings(_) => "duplicate_mappings",
            Self::MissingUnit(_) => "missing_unit",
            Self::MissingMappings(_) => "missing_mappings",
            Self::Request(_) => "request",
            Self::Generation(_) => "generation",
            Self::MergeDuplicateUnit(_) => "merge_duplicate_unit",
            Self::MergeMissingUnit(_) => "merge_missing_unit",
            Self::MergeUnrequestedUnit(_) => "merge_unrequested_unit",
            Self::MergeBackendMismatch(_) => "merge_backend_mismatch",
            Self::MergeCapabilityMismatch(_) => "merge_capability_mismatch",
            Self::MergeTargetMismatch(_) => "merge_target_mismatch",
            Self::MergeMissingArtifact(_) => "merge_missing_artifact",
            Self::MergeUnrequestedArtifact(_) => "merge_unrequested_artifact",
            Self::MergeArtifactKindMismatch(_) => "merge_artifact_kind_mismatch",
            Self::MergeReadFailed { .. } => "merge_read_failed",
            Self::MergeLengthMismatch { .. } => "merge_length_mismatch",
            Self::MergeDigestMismatch { .. } => "merge_digest_mismatch",
            Self::MergeInvalidContent(_) => "merge_invalid_content",
        }
    }
}

impl DiagnosticEmissionStagingFailure {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Incomplete => "incomplete",
            Self::DuplicateContribution(_) => "duplicate_contribution",
            Self::MissingContribution(_) => "missing_contribution",
            Self::InvalidContribution(_) => "invalid_contribution",
            Self::UnexpectedContribution(_) => "unexpected_contribution",
            Self::UnsupportedOutput(_) => "unsupported_output",
            Self::InvalidPath(_) => "invalid_path",
        }
    }
}

impl DiagnosticEmissionLinkPlanFailure {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Incomplete => "incomplete",
            Self::MissingLinkedProduct => "missing_linked_product",
            Self::MissingLinker => "missing_linker",
            Self::UnexpectedLinker => "unexpected_linker",
            Self::DuplicateStagedArtifact(_) => "duplicate_staged_artifact",
            Self::MissingStagedArtifact(_) => "missing_staged_artifact",
            Self::UnexpectedStagedArtifact(_) => "unexpected_staged_artifact",
            Self::UnsupportedStagedArtifact(_) => "unsupported_staged_artifact",
            Self::DuplicateOutputStaging(_) => "duplicate_output_staging",
            Self::MissingOutputStaging(_) => "missing_output_staging",
            Self::UnexpectedOutputStaging(_) => "unexpected_output_staging",
            Self::OutputKindMismatch { .. } => "output_kind_mismatch",
            Self::InvalidStartupInputKind(_) => "invalid_startup_input_kind",
            Self::InvalidNativeInputKind(_) => "invalid_native_input_kind",
            Self::InvalidTerminationInputKind(_) => "invalid_termination_input_kind",
            Self::InputOrdinalOverflow => "input_ordinal_overflow",
            Self::OutputOrdinalOverflow => "output_ordinal_overflow",
            Self::InputEmptyPath => "input_empty_path",
            Self::InputSourceKindMismatch => "input_source_kind_mismatch",
            Self::WholeArchiveRequiresArchive => "whole_archive_requires_archive",
            Self::OutputEmptyPath => "output_empty_path",
            Self::MissingInputs => "missing_inputs",
            Self::DuplicateInput(_) => "duplicate_input",
            Self::DuplicateSearchPath => "duplicate_search_path",
            Self::MissingRuntimeComponent => "missing_runtime_component",
            Self::UnexpectedRuntimeComponent => "unexpected_runtime_component",
            Self::RuntimeArtifactMismatch => "runtime_artifact_mismatch",
            Self::MissingPrimaryOutput => "missing_primary_output",
            Self::MultiplePrimaryOutputs => "multiple_primary_outputs",
            Self::OptionalPrimaryOutput => "optional_primary_output",
            Self::DuplicateOutput(_) => "duplicate_output",
            Self::OutputPathCollision { .. } => "output_path_collision",
            Self::MissingStartupMode => "missing_startup_mode",
            Self::UnexpectedStartupMode => "unexpected_startup_mode",
            Self::MissingStartupInput => "missing_startup_input",
            Self::UnexpectedStartupInput(_) => "unexpected_startup_input",
            Self::MissingDebugCompanion => "missing_debug_companion",
            Self::UnexpectedDebugCompanion => "unexpected_debug_companion",
            Self::IncompatibleOutputKind { .. } => "incompatible_output_kind",
            Self::InvalidOutputPath(_) => "invalid_output_path",
        }
    }
}

impl DiagnosticEmissionEvaluationFailure {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::Cycle(failure) => failure.reason(),
            Self::Runtime(failure) => failure.reason(),
            Self::SemanticValueStoreCreate => "semantic_value_store_create",
            Self::SemanticValue(failure) => failure.as_str(),
            Self::Binding(failure) => failure.as_str(),
            Self::MirCapacity => "mir_capacity_exceeded",
            Self::ConstantCallableBodyUnavailable => "constant_callable_body_unavailable",
            Self::ConstantCallableRootUnavailable => "constant_callable_root_unavailable",
            Self::AtomicRepresentationTypeUnavailable => "atomic_representation_type_unavailable",
            Self::AtomicRepresentationArgumentsUnavailable => {
                "atomic_representation_arguments_unavailable"
            }
            Self::AtomicInitializerArgumentUnavailable => "atomic_initializer_argument_unavailable",
            Self::AtomicInitializerResultUnavailable => "atomic_initializer_result_unavailable",
            Self::UninitInitializerResultUnavailable => "uninit_initializer_result_unavailable",
            Self::ImportedExecutableTemplateMismatch => "imported_executable_template_mismatch",
            Self::SemanticQuery(failure) => failure.as_str(),
            Self::Product(failure) => failure.as_str(),
            Self::Foreign(failure) => failure.as_str(),
            Self::Checker(failure) => failure.as_str(),
        }
    }
}

impl DiagnosticBindingFailure {
    pub const fn as_str(&self) -> &'static str {
        self.reason
    }
}

impl DiagnosticSemanticValueFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CapacityExhausted { .. } => "binding_semantic_value_capacity_exhausted",
        }
    }
}
