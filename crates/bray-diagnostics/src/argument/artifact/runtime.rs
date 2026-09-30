use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates a runtime-artifact metadata or catalog problem argument.
    pub const fn runtime_artifact_problem(problem: DiagnosticRuntimeArtifactProblem) -> Self {
        Self::new(
            DiagnosticArgName::RuntimeArtifactProblem,
            DiagnosticArgValue::RuntimeArtifactProblem(problem),
        )
    }
}

/// Locale-neutral runtime-artifact metadata and catalog failure categories.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticRuntimeArtifactProblem {
    /// Metadata exceeds its fixed decoding resource limit.
    MetadataSizeLimitExceeded,
    /// Metadata does not match the serialized schema.
    MalformedMetadata,
    /// Metadata declares an unsupported format identity or revision.
    UnsupportedFormat,
    /// Metadata declares an invalid runtime identity.
    InvalidRuntimeIdentity,
    /// Metadata declares an invalid artifact identity.
    InvalidArtifactIdentity,
    /// Metadata declares an invalid target identity.
    InvalidTarget,
    /// Metadata declares an invalid panic ABI identity.
    InvalidPanicAbi,
    /// Metadata names a capability outside the closed runtime contract.
    UnknownCapability,
    /// Metadata names a role outside the closed runtime contract.
    UnknownRole,
    /// Metadata names a native platform capability outside the closed contract.
    UnknownPlatformService,
    /// Metadata declares an invalid runtime role symbol.
    InvalidRoleSymbol,
    /// Metadata declares an unknown runtime role implementation boundary.
    UnknownRoleImplementation,
    /// Metadata declares an unknown component purpose.
    UnknownComponentPurpose,
    /// Metadata declares an invalid component identity.
    InvalidComponentIdentity,
    /// Metadata declares an invalid native-index digest.
    InvalidNativeIndexDigest,
    /// A runtime contract publishes one role more than once.
    DuplicateContractRole(String),
    /// A runtime contract assigns a compiler-owned role to the runtime.
    CompilerOwnedRole(String),
    /// A runtime contract omits baseline cooperative execution.
    MissingCooperativeExecution,
    /// Metadata declares an invalid native-index file name.
    InvalidNativeIndexFileName,
    /// Metadata lacks one canonical index per runtime purpose.
    InvalidNativeIndexes,
    /// A native index or payload failed authentication with a specific cause.
    InvalidNativeArtifact(crate::DiagnosticNativeArtifactCause),
    /// A component identity occurs more than once.
    DuplicateComponent(String),
    /// A component claims a role absent from the runtime contract.
    UnknownComponentRole(String),
    /// A component claims a capability absent from the runtime contract.
    UnknownComponentCapability(String),
    /// An ordinary product component claims the test-host entry role.
    TestRoleInProductComponent(String),
    /// A runtime role has no component owner for one product category.
    MissingRoleOwner {
        /// Product category with incomplete ownership.
        purpose: DiagnosticRuntimeArtifactPurpose,
        /// Unowned runtime role.
        role: String,
    },
    /// A runtime role has multiple component owners for one product category.
    DuplicateRoleOwner {
        /// Product category with contradictory ownership.
        purpose: DiagnosticRuntimeArtifactPurpose,
        /// Multiply owned runtime role.
        role: String,
    },
    /// A runtime capability has no component owner for one product category.
    MissingCapabilityOwner {
        /// Product category with incomplete ownership.
        purpose: DiagnosticRuntimeArtifactPurpose,
        /// Unowned runtime capability.
        capability: String,
    },
    /// A runtime capability has multiple component owners for one product category.
    DuplicateCapabilityOwner {
        /// Product category with contradictory ownership.
        purpose: DiagnosticRuntimeArtifactPurpose,
        /// Multiply owned runtime capability.
        capability: String,
    },
    /// A native platform capability is overridden by multiple components.
    DuplicatePlatformServiceOwner {
        /// Product category with contradictory overrides.
        purpose: DiagnosticRuntimeArtifactPurpose,
    },
    /// The runtime target has no supported native representation.
    InvalidNativeTarget,
    /// A selected native index targets another platform.
    IncompatibleIndexTarget,
}

impl DiagnosticRuntimeArtifactProblem {
    /// Returns the stable machine key for this failure category.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::MetadataSizeLimitExceeded => "metadata_size_limit_exceeded",
            Self::MalformedMetadata => "malformed_metadata",
            Self::UnsupportedFormat => "unsupported_format",
            Self::InvalidRuntimeIdentity => "invalid_runtime_identity",
            Self::InvalidArtifactIdentity => "invalid_artifact_identity",
            Self::InvalidTarget => "invalid_target",
            Self::InvalidPanicAbi => "invalid_panic_abi",
            Self::UnknownCapability => "unknown_capability",
            Self::UnknownRole => "unknown_role",
            Self::UnknownPlatformService => "unknown_platform_service",
            Self::InvalidRoleSymbol => "invalid_role_symbol",
            Self::UnknownRoleImplementation => "unknown_role_implementation",
            Self::UnknownComponentPurpose => "unknown_component_purpose",
            Self::InvalidComponentIdentity => "invalid_component_identity",
            Self::InvalidNativeIndexDigest => "invalid_native_index_digest",
            Self::DuplicateContractRole(_) => "duplicate_contract_role",
            Self::CompilerOwnedRole(_) => "compiler_owned_role",
            Self::MissingCooperativeExecution => "missing_cooperative_execution",
            Self::InvalidNativeIndexFileName => "invalid_native_index_file_name",
            Self::InvalidNativeIndexes => "invalid_native_indexes",
            Self::InvalidNativeArtifact(_) => "invalid_native_artifact",
            Self::DuplicateComponent(_) => "duplicate_component",
            Self::UnknownComponentRole(_) => "unknown_component_role",
            Self::UnknownComponentCapability(_) => "unknown_component_capability",
            Self::TestRoleInProductComponent(_) => "test_role_in_product_component",
            Self::MissingRoleOwner { .. } => "missing_role_owner",
            Self::DuplicateRoleOwner { .. } => "duplicate_role_owner",
            Self::MissingCapabilityOwner { .. } => "missing_capability_owner",
            Self::DuplicateCapabilityOwner { .. } => "duplicate_capability_owner",
            Self::DuplicatePlatformServiceOwner { .. } => "duplicate_platform_service_owner",
            Self::InvalidNativeTarget => "invalid_native_target",
            Self::IncompatibleIndexTarget => "incompatible_index_target",
        }
    }
}

/// Product category attached to a runtime-artifact ownership diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticRuntimeArtifactPurpose {
    /// Ordinary executable product runtime surface.
    Product,
    /// Test-runner runtime surface.
    TestRunner,
}

impl DiagnosticRuntimeArtifactPurpose {
    /// Returns the stable machine key for this product category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::TestRunner => "test_runner",
        }
    }
}
