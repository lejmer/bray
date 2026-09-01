use std::path::PathBuf;

use super::checker::DiagnosticCheckerFailure;
use crate::{
    DiagnosticArtifactDigest, DiagnosticArtifactKind, DiagnosticArtifactRequirement,
    DiagnosticAssemblySyntaxKind, DiagnosticDebugInformationMode, DiagnosticDebugOutputMode,
    DiagnosticIoErrorKind, DiagnosticLinkInputKind, DiagnosticLinkedArtifactKind,
    DiagnosticLinkedProductKind, DiagnosticOutputSink, DiagnosticProductKind,
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
    MissingContribution(DiagnosticEmissionArtifact),
    InvalidContribution(DiagnosticEmissionArtifact),
    Publication(DiagnosticEmissionArtifact),
    Linking,
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
    MissingExecutableHost,
    MissingRootFrame(DiagnosticArtifactDigest),
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
    InvalidCompilation,
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
    NonLibraryProduct,
    DependencyCountOverflow,
    DuplicateDependencyPackage(String),
    NonCanonicalSymbolOrder {
        previous: u32,
        current: u32,
    },
    IdentityEmpty,
    IdentitySymbolCountOverflow,
    IdentityNonCanonicalSymbolId {
        expected: u32,
        actual: u32,
    },
    IdentityMissingPackageRoot {
        actual: String,
    },
    IdentityPackageRootHasContainer(u32),
    IdentityPackageMismatch(u32),
    IdentitySymbolKindMismatch {
        record: u32,
        declared: String,
        keyed: String,
    },
    IdentityDuplicateExternalKey {
        first: u32,
        duplicate: u32,
    },
    IdentityMissingContainer(u32),
    IdentityInvalidContainer {
        record: u32,
        container: u32,
    },
    IdentityContainerKeyMismatch {
        record: u32,
        container: u32,
    },
    IdentityUnexpectedRoot {
        record: u32,
        kind: String,
    },
    RelationshipSymbolOutOfBounds {
        owner: u32,
        member: u32,
        ordinal: u32,
    },
    InvalidRelationship {
        owner: u32,
        member: u32,
        ordinal: u32,
    },
    DuplicateRelationshipPosition {
        owner: u32,
        member: u32,
        ordinal: u32,
    },
    ExportOwnerOutOfBounds(u32),
    InvalidExportOwner(u32),
    ExportTargetOutOfBounds(u32),
    DependencyOutOfBounds(u32),
    DependencyKeyPackageMismatch(u32),
    InvalidDirectExportTarget(u32),
    DuplicateExportName {
        owner: u32,
        name: String,
    },
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
    InvalidContent(DiagnosticEmissionArtifact),
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
    RuntimeContractMismatch,
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
    MissingExecutableHost,
    UnexpectedExecutableHost,
    ExecutableHostProductMismatch,
    ExecutableHostTargetMismatch,
    UnexpectedEntryPoint,
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
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticEmissionEvaluationFailure {
    Cycle,
    Infrastructure,
    SemanticValueStoreCreate,
    SemanticValue(DiagnosticSemanticValueFailure),
    Binding(DiagnosticBindingFailure),
    LoweringInput(crate::DiagnosticLoweringInputFailure),
    Lowering(crate::DiagnosticLoweringFailure),
    ConstantCallableBodyUnavailable,
    ConstantCallableRootUnavailable,
    AtomicRepresentationTypeUnavailable,
    AtomicRepresentationArgumentsUnavailable,
    AtomicInitializerArgumentUnavailable,
    AtomicInitializerResultUnavailable,
    UninitInitializerResultUnavailable,
    ImportedExecutableTemplateMismatch,
    SemanticContext,
    /// A semantic query failed with an exact compiler-owned category.
    SemanticQuery(DiagnosticSemanticQueryFailure),
    Checker(DiagnosticCheckerFailure),
}

/// Stable semantic-query failure category retained for product diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSemanticQueryFailure {
    /// A retained semantic relationship violated its compiler contract.
    ContractViolation,
    /// A callable signature violated its structural contract.
    CallableSignature,
    /// A generic substitution violated its declared parameter shape.
    GenericSubstitution,
    /// A bound unit violated its key, tree, or root contract.
    BoundUnit,
    /// Implementation selection or durable evidence was malformed.
    Implementation,
    /// Checked constant-term publication rejected its occurrence input.
    CheckedConstantTerms,
    /// A type-associated surface rejected its member input.
    TypeSurface,
    /// Generated preparsed syntax violated its event contract.
    PreparsedSyntax,
}

/// Exact compiler-owned failure observed while binding one source-level program element.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticBindingFailure {
    DependencyUnavailable,
    InvalidUnitKey,
    MissingSyntax,
    MissingOwner,
    MissingModule,
    InvalidSurfaceName,
    SemanticValue(DiagnosticSemanticValueFailure),
    Construction,
    Binding,
    Assembly,
}

/// Exact canonical-value failure that prevented semantic binding.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSemanticValueFailure {
    ForeignId {
        expected_store: u64,
        actual_store: u64,
    },
    UnknownId {
        kind: &'static str,
    },
    CapacityExhausted {
        kind: &'static str,
    },
    GenericOwnerMismatch {
        expected_kind: &'static str,
        expected: u32,
        actual_kind: &'static str,
        actual: u32,
    },
    OpenSubstitution,
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
            Self::UnexpectedPackageInterfaceArtifact => "unexpected_package_interface_artifact",
            Self::PackageInterfaceProductMismatch { .. } => "package_interface_product_mismatch",
            Self::MissingLinkedProduct => "missing_linked_product",
            Self::MultipleLinkedProducts => "multiple_linked_products",
            Self::LinkedCompanionRequirementMismatch => "linked_companion_requirement_mismatch",
            Self::MissingBackend => "missing_backend",
            Self::MissingCodegenUnits => "missing_codegen_units",
            Self::MissingExecutableHost => "missing_executable_host",
            Self::MissingRootFrame(_) => "missing_root_frame",
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
            Self::SemanticValueStoreCreate => "semantic_value_store_create",
            Self::SemanticValue(_) => "semantic_value",
            Self::RecoveredPublicSymbol(_) => "recovered_public_symbol",
            Self::ConstantCallableEvaluation { .. } => "constant_callable_evaluation",
            Self::ExecutableTemplateEvaluation { .. } => "executable_template_evaluation",
            Self::IncompletePublicDeclaration(_) => "incomplete_public_declaration",
            Self::DuplicateSymbol(_) => "duplicate_symbol",
            Self::MissingSymbol(_) => "missing_symbol",
            Self::SymbolCountOverflow => "symbol_count_overflow",
            Self::NonLibraryProduct => "non_library_product",
            Self::DependencyCountOverflow => "dependency_count_overflow",
            Self::DuplicateDependencyPackage(_) => "duplicate_dependency_package",
            Self::NonCanonicalSymbolOrder { .. } => "non_canonical_symbol_order",
            Self::IdentityEmpty => "identity_empty",
            Self::IdentitySymbolCountOverflow => "identity_symbol_count_overflow",
            Self::IdentityNonCanonicalSymbolId { .. } => "identity_non_canonical_symbol_id",
            Self::IdentityMissingPackageRoot { .. } => "identity_missing_package_root",
            Self::IdentityPackageRootHasContainer(_) => "identity_package_root_has_container",
            Self::IdentityPackageMismatch(_) => "identity_package_mismatch",
            Self::IdentitySymbolKindMismatch { .. } => "identity_symbol_kind_mismatch",
            Self::IdentityDuplicateExternalKey { .. } => "identity_duplicate_external_key",
            Self::IdentityMissingContainer(_) => "identity_missing_container",
            Self::IdentityInvalidContainer { .. } => "identity_invalid_container",
            Self::IdentityContainerKeyMismatch { .. } => "identity_container_key_mismatch",
            Self::IdentityUnexpectedRoot { .. } => "identity_unexpected_root",
            Self::RelationshipSymbolOutOfBounds { .. } => "relationship_symbol_out_of_bounds",
            Self::InvalidRelationship { .. } => "invalid_relationship",
            Self::DuplicateRelationshipPosition { .. } => "duplicate_relationship_position",
            Self::ExportOwnerOutOfBounds(_) => "export_owner_out_of_bounds",
            Self::InvalidExportOwner(_) => "invalid_export_owner",
            Self::ExportTargetOutOfBounds(_) => "export_target_out_of_bounds",
            Self::DependencyOutOfBounds(_) => "dependency_out_of_bounds",
            Self::DependencyKeyPackageMismatch(_) => "dependency_key_package_mismatch",
            Self::InvalidDirectExportTarget(_) => "invalid_direct_export_target",
            Self::DuplicateExportName { .. } => "duplicate_export_name",
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
            Self::InvalidContent(_) => "invalid_content",
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
            Self::RuntimeContractMismatch => "runtime_contract_mismatch",
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
            Self::MissingExecutableHost => "missing_executable_host",
            Self::UnexpectedExecutableHost => "unexpected_executable_host",
            Self::ExecutableHostProductMismatch => "executable_host_product_mismatch",
            Self::ExecutableHostTargetMismatch => "executable_host_target_mismatch",
            Self::UnexpectedEntryPoint => "unexpected_entry_point",
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
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cycle => "cycle",
            Self::Infrastructure => "infrastructure",
            Self::SemanticValueStoreCreate => "semantic_value_store_create",
            Self::SemanticValue(failure) => failure.as_str(),
            Self::Binding(failure) => failure.as_str(),
            Self::LoweringInput(failure) => failure.as_str(),
            Self::Lowering(failure) => failure.as_str(),
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
            Self::SemanticContext => "semantic_context",
            Self::SemanticQuery(failure) => failure.as_str(),
            Self::Checker(failure) => failure.as_str(),
        }
    }
}

impl DiagnosticSemanticQueryFailure {
    /// Returns this failure category's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContractViolation => "semantic_query_contract_violation",
            Self::CallableSignature => "semantic_query_callable_signature",
            Self::GenericSubstitution => "semantic_query_generic_substitution",
            Self::BoundUnit => "semantic_query_bound_unit",
            Self::Implementation => "semantic_query_implementation",
            Self::CheckedConstantTerms => "semantic_query_checked_constant_terms",
            Self::TypeSurface => "semantic_query_type_surface",
            Self::PreparsedSyntax => "semantic_query_preparsed_syntax",
        }
    }
}

impl DiagnosticBindingFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DependencyUnavailable => "binding_dependency_unavailable",
            Self::InvalidUnitKey => "binding_invalid_unit_key",
            Self::MissingSyntax => "binding_missing_syntax",
            Self::MissingOwner => "binding_missing_owner",
            Self::MissingModule => "binding_missing_module",
            Self::InvalidSurfaceName => "binding_invalid_surface_name",
            Self::SemanticValue(failure) => failure.as_str(),
            Self::Construction => "binding_construction",
            Self::Binding => "binding_recovery_root",
            Self::Assembly => "binding_assembly",
        }
    }
}

impl DiagnosticSemanticValueFailure {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ForeignId { .. } => "binding_semantic_value_foreign_id",
            Self::UnknownId { .. } => "binding_semantic_value_unknown_id",
            Self::CapacityExhausted { .. } => "binding_semantic_value_capacity_exhausted",
            Self::GenericOwnerMismatch { .. } => "binding_semantic_value_generic_owner_mismatch",
            Self::OpenSubstitution => "binding_semantic_value_open_substitution",
        }
    }
}
