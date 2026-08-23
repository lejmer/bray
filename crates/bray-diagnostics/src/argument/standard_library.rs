use super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact standard library manifest contract problem argument.
    pub const fn standard_library_manifest_problem(
        problem: DiagnosticStandardLibraryManifestProblem,
    ) -> Self {
        Self::new(
            DiagnosticArgName::StandardLibraryManifestProblem,
            DiagnosticArgValue::StandardLibraryManifestProblem(problem),
        )
    }
}

/// Locale-neutral standard library manifest contract violation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStandardLibraryManifestProblem {
    Malformed,
    NonCanonicalEncoding,
    InvalidDigest,
    InvalidInterfaceArtifact,
    InvalidImplementationArtifact,
    InvalidTargetArtifact,
    InvalidArtifactPath,
    MissingArtifact,
    DuplicateArtifact,
    DuplicateArtifactPath,
    MissingTarget,
    DuplicateTarget,
    InvalidIdentity,
    InvalidNativeLink,
    InvalidPlatformServices,
    DuplicatePlatformService,
    InvalidOptimizationMetadata(DiagnosticStandardLibraryOptimizationMetadataProblem),
    InvalidOptimizationFallback,
    BundleDigestMismatch,
    LengthExceeded,
}

impl DiagnosticStandardLibraryManifestProblem {
    /// Returns the stable machine key for this contract violation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::NonCanonicalEncoding => "non_canonical_encoding",
            Self::InvalidDigest => "invalid_digest",
            Self::InvalidInterfaceArtifact => "invalid_interface_artifact",
            Self::InvalidImplementationArtifact => "invalid_implementation_artifact",
            Self::InvalidTargetArtifact => "invalid_target_artifact",
            Self::InvalidArtifactPath => "invalid_artifact_path",
            Self::MissingArtifact => "missing_artifact",
            Self::DuplicateArtifact => "duplicate_artifact",
            Self::DuplicateArtifactPath => "duplicate_artifact_path",
            Self::MissingTarget => "missing_target",
            Self::DuplicateTarget => "duplicate_target",
            Self::InvalidIdentity => "invalid_identity",
            Self::InvalidNativeLink => "invalid_native_link",
            Self::InvalidPlatformServices => "invalid_platform_services",
            Self::DuplicatePlatformService => "duplicate_platform_service",
            Self::InvalidOptimizationMetadata(problem) => problem.as_str(),
            Self::InvalidOptimizationFallback => "invalid_optimization_fallback",
            Self::BundleDigestMismatch => "bundle_digest_mismatch",
            Self::LengthExceeded => "length_exceeded",
        }
    }
}

/// Locale-neutral native optimization metadata contract violation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStandardLibraryOptimizationMetadataProblem {
    MissingForArchive,
    AttachedToUnsupportedArtifact,
    UnsupportedSemantics,
    UnsupportedProducerKind,
    MissingProducerImplementation,
    MissingProducerImplementationRevision,
    MissingToolchain,
    MissingToolchainRevision,
    MissingTargetTriple,
    MissingDataLayout,
    UnsupportedRelocationModel,
    UnsupportedCodeModel,
    NonCanonicalFallbackPath,
    ZeroModuleCount,
    InvalidPreservationRoot,
    UnsupportedLifecycleRoot,
    UnknownPlatformService,
    NonCanonicalDependencyPath,
    InvalidPartition,
    DuplicatePartition,
    RuntimeAbiMismatch,
    TargetMismatch,
    CompatibilityMismatch,
    ToolchainMismatch,
    MissingDependencyArtifact,
    MissingBrayPartition,
}

impl DiagnosticStandardLibraryOptimizationMetadataProblem {
    /// Returns the stable machine key for this contract violation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MissingForArchive => "optimization_metadata_missing_for_archive",
            Self::AttachedToUnsupportedArtifact => {
                "optimization_metadata_attached_to_unsupported_artifact"
            }
            Self::UnsupportedSemantics => "optimization_metadata_unsupported_semantics",
            Self::UnsupportedProducerKind => "optimization_metadata_unsupported_producer_kind",
            Self::MissingProducerImplementation => {
                "optimization_metadata_missing_producer_implementation"
            }
            Self::MissingProducerImplementationRevision => {
                "optimization_metadata_missing_producer_implementation_revision"
            }
            Self::MissingToolchain => "optimization_metadata_missing_toolchain",
            Self::MissingToolchainRevision => "optimization_metadata_missing_toolchain_revision",
            Self::MissingTargetTriple => "optimization_metadata_missing_target_triple",
            Self::MissingDataLayout => "optimization_metadata_missing_data_layout",
            Self::UnsupportedRelocationModel => {
                "optimization_metadata_unsupported_relocation_model"
            }
            Self::UnsupportedCodeModel => "optimization_metadata_unsupported_code_model",
            Self::NonCanonicalFallbackPath => "optimization_metadata_non_canonical_fallback_path",
            Self::ZeroModuleCount => "optimization_metadata_zero_module_count",
            Self::InvalidPreservationRoot => "optimization_metadata_invalid_preservation_root",
            Self::UnsupportedLifecycleRoot => "optimization_metadata_unsupported_lifecycle_root",
            Self::UnknownPlatformService => "optimization_metadata_unknown_platform_service",
            Self::NonCanonicalDependencyPath => {
                "optimization_metadata_non_canonical_dependency_path"
            }
            Self::InvalidPartition => "optimization_metadata_invalid_partition",
            Self::DuplicatePartition => "optimization_metadata_duplicate_partition",
            Self::RuntimeAbiMismatch => "optimization_metadata_runtime_abi_mismatch",
            Self::TargetMismatch => "optimization_metadata_target_mismatch",
            Self::CompatibilityMismatch => "optimization_metadata_compatibility_mismatch",
            Self::ToolchainMismatch => "optimization_metadata_toolchain_mismatch",
            Self::MissingDependencyArtifact => "optimization_metadata_missing_dependency_artifact",
            Self::MissingBrayPartition => "optimization_metadata_missing_bray_partition",
        }
    }
}
