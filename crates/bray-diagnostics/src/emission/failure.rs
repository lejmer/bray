use std::path::PathBuf;

use crate::{
    DiagnosticArtifactDigest, DiagnosticArtifactKind, DiagnosticAssemblySyntaxKind,
    DiagnosticDebugInformationMode, DiagnosticDebugOutputMode, DiagnosticFactRuntimeFailure,
    DiagnosticIoErrorKind, DiagnosticLinkInputKind, DiagnosticLinkedArtifactKind,
    DiagnosticLinkedProductKind, DiagnosticOutputSink, DiagnosticProductKind,
    DiagnosticProductQueryFailure,
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
    /// The published native package index exceeds its supported size.
    NativeIndexSizeLimitExceeded,
    NativeRead {
        path: std::path::PathBuf,
        kind: DiagnosticIoErrorKind,
    },
    LinkPlan(DiagnosticEmissionLinkPlanFailure),
    Evaluation(DiagnosticEmissionEvaluationFailure),
    /// Test-catalog encoding failed before the output could be published.
    TestCatalogCapacityExceeded,
    MissingContribution(DiagnosticEmissionArtifact),
    InvalidContribution(DiagnosticEmissionArtifact),
    Publication(DiagnosticEmissionArtifact),
    Linking,
    // rust-style: broad-failure
    IncompleteProduct,
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
    DeclarationDiscoveryFailure {
        cause: DiagnosticEmissionEvaluationFailure,
        cycle: Box<[String]>,
    },
    ImplementationContentTooLarge,
}

/// Exact code-generation or contribution-merge contract that failed.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticEmissionCodegenFailure {
    Incomplete,
    Request(DiagnosticArtifactDigest),
    Generation(DiagnosticArtifactDigest),
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
    /// A semantic query failed with an exact compiler-owned category.
    SemanticQuery(DiagnosticSemanticQueryFailure),
    /// Product specialization or realization violated an exact retained contract.
    Product(DiagnosticProductQueryFailure),
    /// Foreign-boundary construction violated an exact retained contract.
    Foreign(crate::DiagnosticForeignQueryFailure),
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
            Self::NativeIndexSizeLimitExceeded => "native_index",
            Self::NativeRead { .. } => "native_read",
            Self::LinkPlan(_) => "link_plan",
            Self::Evaluation(_) => "evaluation",
            Self::TestCatalogCapacityExceeded => "test_catalog_capacity_exceeded",
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
            Self::NativeIndexSizeLimitExceeded => "size_limit_exceeded",
            Self::NativeRead { .. } => "read",
            Self::LinkPlan(failure) => failure.as_str(),
            Self::Evaluation(failure) => failure.as_str(),
            Self::TestCatalogCapacityExceeded => "test_catalog_capacity_exceeded",
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
            Self::DeclarationDiscoveryFailure { .. } => "declaration_discovery_failure",
            Self::ImplementationContentTooLarge => "implementation_content_too_large",
        }
    }
}

impl DiagnosticEmissionCodegenFailure {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Incomplete => "incomplete",
            Self::Request(_) => "request",
            Self::Generation(_) => "generation",
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
            Self::SemanticQuery(failure) => failure.as_str(),
            Self::Product(failure) => failure.as_str(),
            Self::Foreign(failure) => failure.as_str(),
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
