use std::collections::BTreeSet;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::{StableDigestHasher, is_canonical_relative_path, sorted_unique_shared_slice};
use bray_runtime_interface::{PlatformServiceRole, RuntimeAbiVersion};
use bray_symbols::NativeLinkRequirement;
use bray_target::TargetIdentity;

use super::wire::encode_payload;
use super::StandardLibraryOptimizationMetadata;

/// Fixed bundle manifest file name beneath a configured standard library root.
pub const STANDARD_LIBRARY_MANIFEST_FILE_NAME: &str = "manifest.json";
const STANDARD_LIBRARY_INTERFACE_FILE_NAME: &str = "std.brayi";
const STANDARD_LIBRARY_IMPLEMENTATION_FILE_NAME: &str = "std.brayimpl";

/// Returns the canonical portable artifact directory for a target and runtime ABI.
pub fn standard_library_target_artifact_directory(
    target: &TargetIdentity,
    runtime_abi: RuntimeAbiVersion,
) -> String {
    format!(
        "targets/{}/{}.{}",
        target.as_str(),
        runtime_abi.major(),
        runtime_abi.minor()
    )
}

/// BLAKE3 digest of one complete standard library artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StandardLibraryArtifactDigest([u8; 32]);

impl StandardLibraryArtifactDigest {
    /// Computes the digest of exact artifact bytes.
    pub fn for_bytes(bytes: &[u8]) -> Self {
        let mut hasher = StableDigestHasher::new();

        hasher.write(bytes);

        Self(hasher.finalize())
    }

    /// Creates a digest from exact BLAKE3 bytes.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact digest bytes.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Domain-separated identity of one complete standard library bundle manifest.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StandardLibraryBundleDigest([u8; 32]);

impl StandardLibraryBundleDigest {
    /// Creates a bundle digest from exact BLAKE3 bytes.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(super) fn for_payload(payload: &[u8]) -> Result<Self, StandardLibraryManifestError> {
        let payload_len = u64::try_from(payload.len())
            .map_err(|_| StandardLibraryManifestError::LengthExceeded)?;

        let mut hasher = StableDigestHasher::new();

        hasher.write(b"bray.standard-library.bundle\0");
        hasher.write_u32(1);
        hasher.write_u64(payload_len);
        hasher.write(payload);

        Ok(Self(hasher.finalize()))
    }

    /// Returns the exact digest bytes.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Artifact category stored in a standard library bundle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StandardLibraryArtifactKind {
    /// Public compiled package interface.
    PackageInterface,
    /// Generic executable and const-evaluation implementation payloads.
    PackageImplementation,
    /// Compiler-owned dependency metadata.
    DependencyMetadata,
    /// Relocatable native object.
    RelocatableObject,
    /// Native static library.
    StaticLibrary,
    /// Native platform-service provider archive.
    PlatformServiceLibrary,
    /// Summary-bearing LLVM modules for one independently selectable native partition.
    OptimizationArchive,
    /// Native shared library.
    SharedLibrary,
    /// Private runtime artifact metadata.
    RuntimeArtifact,
}

impl StandardLibraryArtifactKind {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::PackageInterface => "package_interface",
            Self::PackageImplementation => "package_implementation",
            Self::DependencyMetadata => "dependency_metadata",
            Self::RelocatableObject => "relocatable_object",
            Self::StaticLibrary => "static_library",
            Self::PlatformServiceLibrary => "platform_service_library",
            Self::OptimizationArchive => "optimization_archive",
            Self::SharedLibrary => "shared_library",
            Self::RuntimeArtifact => "runtime_artifact",
        }
    }

    pub(super) fn for_str(value: &str) -> Option<Self> {
        match value {
            "package_interface" => Some(Self::PackageInterface),
            "package_implementation" => Some(Self::PackageImplementation),
            "dependency_metadata" => Some(Self::DependencyMetadata),
            "relocatable_object" => Some(Self::RelocatableObject),
            "static_library" => Some(Self::StaticLibrary),
            "platform_service_library" => Some(Self::PlatformServiceLibrary),
            "optimization_archive" => Some(Self::OptimizationArchive),
            "shared_library" => Some(Self::SharedLibrary),
            "runtime_artifact" => Some(Self::RuntimeArtifact),
            _ => None,
        }
    }

    const fn accepts_native_links(self) -> bool {
        matches!(
            self,
            Self::RelocatableObject
                | Self::StaticLibrary
                | Self::PlatformServiceLibrary
                | Self::OptimizationArchive
        )
    }
}

/// One immutable file selected by the standard library bundle manifest.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StandardLibraryArtifact {
    kind: StandardLibraryArtifactKind,
    path: Arc<str>,
    byte_len: u64,
    digest: StandardLibraryArtifactDigest,
    platform_services: Arc<[PlatformServiceRole]>,
    native_links: Arc<[NativeLinkRequirement]>,
    optimization: Option<StandardLibraryOptimizationMetadata>,
}

impl StandardLibraryArtifact {
    /// Creates an artifact record from validated metadata.
    pub fn try_new(
        kind: StandardLibraryArtifactKind,
        path: impl Into<Arc<str>>,
        byte_len: u64,
        digest: StandardLibraryArtifactDigest,
    ) -> Result<Self, StandardLibraryManifestError> {
        let path = path.into();

        if !is_canonical_relative_path(&path) {
            return Err(StandardLibraryManifestError::InvalidArtifactPath);
        }

        Ok(Self {
            kind,
            path,
            byte_len,
            digest,
            platform_services: Arc::from([]),
            native_links: Arc::from([]),
            optimization: None,
        })
    }

    /// Returns this platform archive with its exact provided service roles.
    pub fn with_platform_services(
        mut self,
        platform_services: impl IntoIterator<Item = PlatformServiceRole>,
    ) -> Self {
        self.platform_services = platform_services
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .into();

        self
    }

    /// Returns this native link input with its exact library and framework requirements.
    pub fn with_native_links(
        mut self,
        native_links: impl IntoIterator<Item = NativeLinkRequirement>,
    ) -> Self {
        self.native_links = sorted_unique_shared_slice(native_links);

        self
    }

    /// Returns this archive with its cross-artifact optimization contract.
    pub fn with_optimization(mut self, optimization: StandardLibraryOptimizationMetadata) -> Self {
        self.optimization = Some(optimization);

        self
    }

    /// Creates an artifact record and digest from complete bytes.
    pub fn try_for_bytes(
        kind: StandardLibraryArtifactKind,
        path: impl Into<Arc<str>>,
        bytes: &[u8],
    ) -> Result<Self, StandardLibraryManifestError> {
        let byte_len =
            u64::try_from(bytes.len()).map_err(|_| StandardLibraryManifestError::LengthExceeded)?;

        Self::try_new(
            kind,
            path,
            byte_len,
            StandardLibraryArtifactDigest::for_bytes(bytes),
        )
    }

    /// Returns the artifact category.
    pub const fn kind(&self) -> StandardLibraryArtifactKind {
        self.kind
    }

    /// Returns the portable path beneath the bundle root.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the exact artifact byte length.
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// Returns the exact artifact digest.
    pub const fn digest(&self) -> StandardLibraryArtifactDigest {
        self.digest
    }

    /// Returns the exact platform services implemented by this archive.
    pub fn platform_services(&self) -> &[PlatformServiceRole] {
        &self.platform_services
    }

    /// Returns native libraries and frameworks required by this link input.
    pub fn native_links(&self) -> &[NativeLinkRequirement] {
        &self.native_links
    }

    /// Returns the selection and preservation contract for an optimization archive.
    pub const fn optimization(&self) -> Option<&StandardLibraryOptimizationMetadata> {
        self.optimization.as_ref()
    }

    /// Resolves the portable path beneath a configured bundle root.
    pub fn beneath(&self, root: &Path) -> PathBuf {
        self.path
            .split('/')
            .fold(root.to_path_buf(), |path, component| path.join(component))
    }
}

/// Exact target artifact set in one standard library bundle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardLibraryTargetArtifacts {
    target: TargetIdentity,
    runtime_abi: RuntimeAbiVersion,
    artifacts: Arc<[StandardLibraryArtifact]>,
}

impl StandardLibraryTargetArtifacts {
    /// Creates one target set in canonical artifact order.
    pub fn try_new(
        target: TargetIdentity,
        runtime_abi: RuntimeAbiVersion,
        artifacts: impl IntoIterator<Item = StandardLibraryArtifact>,
    ) -> Result<Self, StandardLibraryManifestError> {
        let mut artifacts: Vec<_> = artifacts.into_iter().collect();

        artifacts.sort_unstable();

        if artifacts.is_empty() {
            return Err(StandardLibraryManifestError::MissingArtifact);
        }

        if artifacts.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(StandardLibraryManifestError::DuplicateArtifact);
        }

        if artifacts.iter().any(|artifact| {
            (artifact.kind() == StandardLibraryArtifactKind::PlatformServiceLibrary)
                != !artifact.platform_services().is_empty()
        }) {
            return Err(StandardLibraryManifestError::InvalidPlatformServices);
        }

        if artifacts.iter().any(|artifact| {
            !artifact.kind().accepts_native_links() && !artifact.native_links().is_empty()
        }) {
            return Err(StandardLibraryManifestError::InvalidNativeLink);
        }

        if artifacts.iter().any(|artifact| {
            artifact.kind() == StandardLibraryArtifactKind::OptimizationArchive
                && artifact.optimization().is_none()
        }) {
            return Err(invalid_optimization_metadata(
                StandardLibraryOptimizationMetadataProblem::MissingForArchive,
            ));
        }

        if artifacts.iter().any(|artifact| {
            artifact.kind() != StandardLibraryArtifactKind::OptimizationArchive
                && artifact.optimization().is_some()
        }) {
            return Err(invalid_optimization_metadata(
                StandardLibraryOptimizationMetadataProblem::AttachedToUnsupportedArtifact,
            ));
        }

        let mut platform_services = BTreeSet::new();

        if artifacts
            .iter()
            .flat_map(StandardLibraryArtifact::platform_services)
            .any(|role| !platform_services.insert(*role))
        {
            return Err(StandardLibraryManifestError::DuplicatePlatformService);
        }

        super::optimization::validate_target(&artifacts, &target, runtime_abi)?;

        let prefix = format!(
            "{}/",
            standard_library_target_artifact_directory(&target, runtime_abi)
        );

        if artifacts.iter().any(|artifact| {
            artifact
                .path()
                .strip_prefix(&prefix)
                .is_none_or(|file_name| file_name.is_empty() || file_name.contains('/'))
        }) {
            return Err(StandardLibraryManifestError::InvalidTargetArtifact);
        }

        let interface_path = format!("{prefix}{STANDARD_LIBRARY_INTERFACE_FILE_NAME}");

        let implementation_path = format!("{prefix}{STANDARD_LIBRARY_IMPLEMENTATION_FILE_NAME}");

        if !has_exact_artifact(
            &artifacts,
            StandardLibraryArtifactKind::PackageInterface,
            &interface_path,
        ) {
            return Err(StandardLibraryManifestError::InvalidInterfaceArtifact);
        }

        if !has_exact_artifact(
            &artifacts,
            StandardLibraryArtifactKind::PackageImplementation,
            &implementation_path,
        ) {
            return Err(StandardLibraryManifestError::InvalidImplementationArtifact);
        }

        Ok(Self {
            target,
            runtime_abi,
            artifacts: artifacts.into(),
        })
    }

    /// Returns the exact target identity.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns the required runtime ABI.
    pub const fn runtime_abi(&self) -> RuntimeAbiVersion {
        self.runtime_abi
    }

    /// Returns target artifacts in canonical order.
    pub fn artifacts(&self) -> &[StandardLibraryArtifact] {
        &self.artifacts
    }

    /// Returns the package interface selected for this target.
    pub fn package_interface(&self) -> &StandardLibraryArtifact {
        required_artifact(
            &self.artifacts,
            StandardLibraryArtifactKind::PackageInterface,
        )
    }

    /// Returns the package implementation selected for this target.
    pub fn package_implementation(&self) -> &StandardLibraryArtifact {
        required_artifact(
            &self.artifacts,
            StandardLibraryArtifactKind::PackageImplementation,
        )
    }
}

/// Complete immutable standard library bundle manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardLibraryBundleManifest {
    targets: Arc<[StandardLibraryTargetArtifacts]>,
    bundle_digest: StandardLibraryBundleDigest,
}

impl StandardLibraryBundleManifest {
    /// Creates a manifest, validates its closed inventory, and derives its bundle identity.
    pub fn try_new(
        targets: impl IntoIterator<Item = StandardLibraryTargetArtifacts>,
    ) -> Result<Self, StandardLibraryManifestError> {
        let mut targets: Vec<_> = targets.into_iter().collect();

        targets.sort_unstable_by(|left, right| left.target().cmp(right.target()));

        if targets.is_empty() {
            return Err(StandardLibraryManifestError::MissingTarget);
        }

        if targets
            .windows(2)
            .any(|pair| pair[0].target() == pair[1].target())
        {
            return Err(StandardLibraryManifestError::DuplicateTarget);
        }

        let mut paths = BTreeSet::new();

        for artifact in targets.iter().flat_map(|target| target.artifacts()) {
            if !paths.insert(artifact.path()) {
                return Err(StandardLibraryManifestError::DuplicateArtifactPath);
            }
        }

        let targets: Arc<[_]> = targets.into();
        let payload = encode_payload(&targets)?;
        let bundle_digest = StandardLibraryBundleDigest::for_payload(&payload)?;

        Ok(Self {
            targets,
            bundle_digest,
        })
    }

    /// Returns target sets in canonical target order.
    pub fn targets(&self) -> &[StandardLibraryTargetArtifacts] {
        &self.targets
    }

    /// Returns the derived identity of the complete canonical manifest payload.
    pub const fn bundle_digest(&self) -> StandardLibraryBundleDigest {
        self.bundle_digest
    }
}

fn has_exact_artifact(
    artifacts: &[StandardLibraryArtifact],
    kind: StandardLibraryArtifactKind,
    path: &str,
) -> bool {
    let mut matches = artifacts.iter().filter(|artifact| artifact.kind() == kind);

    matches
        .next()
        .is_some_and(|artifact| artifact.path() == path)
        && matches.next().is_none()
}

fn required_artifact(
    artifacts: &[StandardLibraryArtifact],
    kind: StandardLibraryArtifactKind,
) -> &StandardLibraryArtifact {
    // Construction validates both required package artifacts before storing the inventory.
    artifacts
        .iter()
        .find(|artifact| artifact.kind() == kind)
        .unwrap_or_else(|| panic!("validated target inventory must contain {kind:?}"))
}

/// A standard library bundle manifest violates its semantic or wire contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StandardLibraryManifestError {
    /// Serialized JSON is malformed or does not match the strict schema.
    Malformed,
    /// Serialized JSON is valid but not the canonical compact encoding.
    NonCanonicalEncoding,
    /// A digest is malformed.
    InvalidDigest,
    /// A target interface artifact is missing, duplicated, or has the wrong path.
    InvalidInterfaceArtifact,
    /// A target implementation artifact is missing, duplicated, or has the wrong path.
    InvalidImplementationArtifact,
    /// A target artifact is outside its exact target and runtime ABI directory.
    InvalidTargetArtifact,
    /// An artifact path is not canonical and relative.
    InvalidArtifactPath,
    /// No artifact was supplied where one is required.
    MissingArtifact,
    /// A target artifact record is repeated.
    DuplicateArtifact,
    /// An artifact path is repeated across the closed inventory.
    DuplicateArtifactPath,
    /// No target artifact set was supplied.
    MissingTarget,
    /// A target artifact set is repeated.
    DuplicateTarget,
    /// An identity in the serialized manifest is not the canonical standard library identity.
    InvalidIdentity,
    /// A target-native link requirement is malformed.
    InvalidNativeLink,
    /// Platform-service roles are missing from a provider archive or attached to another artifact.
    InvalidPlatformServices,
    /// A platform-service role is implemented by more than one provider archive.
    DuplicatePlatformService,
    /// Optimization metadata violates its semantic or wire contract.
    InvalidOptimizationMetadata(StandardLibraryOptimizationMetadataProblem),
    /// An optimization artifact does not name its exact compatible object-only fallback.
    InvalidOptimizationFallback,
    /// The published bundle digest does not match the canonical payload.
    BundleDigestMismatch,
    /// A platform length cannot fit the manifest contract.
    LengthExceeded,
}

/// Exact contract violation within native optimization metadata.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StandardLibraryOptimizationMetadataProblem {
    /// An optimization archive has no optimization contract.
    MissingForArchive,
    /// An artifact other than an optimization archive carries an optimization contract.
    AttachedToUnsupportedArtifact,
    /// The optimization semantics are not supported.
    UnsupportedSemantics,
    /// The producer category is not supported.
    UnsupportedProducerKind,
    /// The producer implementation identity is empty.
    MissingProducerImplementation,
    /// The producer implementation revision is empty.
    MissingProducerImplementationRevision,
    /// The producer toolchain identity is empty.
    MissingToolchain,
    /// The producer toolchain revision is empty.
    MissingToolchainRevision,
    /// The target triple is empty.
    MissingTargetTriple,
    /// The LLVM data layout is empty.
    MissingDataLayout,
    /// The relocation model is not supported.
    UnsupportedRelocationModel,
    /// The code model is not supported.
    UnsupportedCodeModel,
    /// The fallback path is not canonical and relative.
    NonCanonicalFallbackPath,
    /// The archive declares no LLVM modules.
    ZeroModuleCount,
    /// A preservation root is not a valid native symbol.
    InvalidPreservationRoot,
    /// A lifecycle root is not supported.
    UnsupportedLifecycleRoot,
    /// A platform-service role is unknown.
    UnknownPlatformService,
    /// A dependency path is not canonical and relative.
    NonCanonicalDependencyPath,
    /// The partition identity is empty or malformed.
    InvalidPartition,
    /// A partition identity appears more than once in the target inventory.
    DuplicatePartition,
    /// The optimization contract names a different runtime ABI.
    RuntimeAbiMismatch,
    /// The optimization contract names a different target.
    TargetMismatch,
    /// Optimization archives for one target use incompatible target settings.
    CompatibilityMismatch,
    /// Optimization archives for one target use different toolchains.
    ToolchainMismatch,
    /// A dependency reference does not resolve to packaged metadata.
    MissingDependencyArtifact,
    /// The target inventory has no Bray standard library partition.
    MissingBrayPartition,
}

pub(super) const fn invalid_optimization_metadata(
    problem: StandardLibraryOptimizationMetadataProblem,
) -> StandardLibraryManifestError {
    StandardLibraryManifestError::InvalidOptimizationMetadata(problem)
}

/// Stable optimization bytes shared by standard-library consumer tests.
#[cfg(any(test, feature = "test-support"))]
pub(crate) const TEST_OPTIMIZATION_ARTIFACT_BYTES: &[u8] = b"test standard library optimization";

/// Completes a minimal target artifact set for tests of standard-library consumers.
#[cfg(any(test, feature = "test-support"))]
pub fn target_artifacts_for_test(
    target: TargetIdentity,
    runtime_abi: RuntimeAbiVersion,
    mut artifacts: Vec<StandardLibraryArtifact>,
) -> Result<StandardLibraryTargetArtifacts, StandardLibraryManifestError> {
    if artifacts
        .iter()
        .any(|artifact| artifact.kind() == StandardLibraryArtifactKind::OptimizationArchive)
    {
        return StandardLibraryTargetArtifacts::try_new(target, runtime_abi, artifacts);
    }

    let prefix = standard_library_target_artifact_directory(&target, runtime_abi);

    let fallback = match artifacts
        .iter()
        .find(|artifact| artifact.kind() == StandardLibraryArtifactKind::StaticLibrary)
    {
        Some(fallback) => fallback.clone(),
        None => {
            let fallback = StandardLibraryArtifact::try_for_bytes(
                StandardLibraryArtifactKind::StaticLibrary,
                format!("{prefix}/std.fallback"),
                b"test standard library fallback",
            )?;

            artifacts.push(fallback.clone());

            fallback
        }
    };

    let producer = super::StandardLibraryOptimizationProducer::try_new(
        super::StandardLibraryOptimizationProducerKind::Bray,
        "test-bray",
        "1",
        "test-llvm",
        "1",
    )?;

    let compatibility = super::StandardLibraryOptimizationCompatibility::try_new(
        target.as_str(),
        "test-data-layout",
        bray_target::RelocationModel::PositionIndependent,
        bray_target::CodeModel::Small,
        runtime_abi,
    )?;

    let fallback =
        super::StandardLibraryOptimizationFallback::try_new(fallback.path(), fallback.digest())?;

    let optimization = super::StandardLibraryOptimizationMetadata::try_new(
        "std",
        producer,
        compatibility,
        fallback,
        std::num::NonZeroU32::MIN,
    )?;

    let optimization = StandardLibraryArtifact::try_for_bytes(
        StandardLibraryArtifactKind::OptimizationArchive,
        format!("{prefix}/std.optimization"),
        TEST_OPTIMIZATION_ARTIFACT_BYTES,
    )?
    .with_optimization(optimization);

    artifacts.push(optimization);

    StandardLibraryTargetArtifacts::try_new(target, runtime_abi, artifacts)
}
