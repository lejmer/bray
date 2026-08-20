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
    InvalidOptimizationMetadata,
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
            Self::InvalidOptimizationMetadata => "invalid_optimization_metadata",
            Self::InvalidOptimizationFallback => "invalid_optimization_fallback",
            Self::BundleDigestMismatch => "bundle_digest_mismatch",
            Self::LengthExceeded => "length_exceeded",
        }
    }
}
