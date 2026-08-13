use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use bray_base::{NonEmptySharedStr, decode_lowercase_hex, lowercase_hex, shared_slice};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::TargetIdentity;
use serde::{Deserialize, Serialize};

use crate::{
    BinarySymbolName, PanicAbiIdentity, ProtectedFrameAbiOperation, ProtectedFrameAbiVersions,
    PlatformServiceRole, RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactId, RuntimeCapability,
    RuntimeContract, RuntimeContractBuildError, RuntimeIdentity, RuntimeRoleBinding,
    RuntimeRoleImplementation,
};

const FORMAT: &str = "bray_native_runtime";
const FORMAT_VERSION: u16 = 2;
const MAXIMUM_METADATA_BYTES: usize = 64 * 1024;

/// Content digest of one packaged native runtime archive.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeArtifactDigest([u8; 32]);

impl RuntimeArtifactDigest {
    /// Creates a digest from its exact bytes.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact digest bytes.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }

    /// Returns the lowercase hexadecimal digest.
    pub fn to_hex(self) -> String {
        lowercase_hex(&self.0)
    }

    fn from_hex(value: &str) -> Option<Self> {
        decode_lowercase_hex(value).map(Self)
    }
}

/// Product category used to select one non-overlapping runtime implementation surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeArtifactPurpose {
    /// Ordinary executable products.
    Product,
    /// Test executables with the native test-host protocol.
    TestRunner,
}

impl RuntimeArtifactPurpose {
    /// Every runtime artifact purpose in canonical order.
    pub const ALL: [Self; 2] = [Self::Product, Self::TestRunner];

    /// Returns the stable metadata name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::TestRunner => "test_runner",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|purpose| purpose.as_str() == name)
    }
}

/// One physical archive and the exact runtime surface it owns for one product category.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifactComponentMetadata {
    identity: RuntimeArtifactId,
    purpose: RuntimeArtifactPurpose,
    roles: Arc<[RuntimeAbiRole]>,
    capabilities: Arc<[RuntimeCapability]>,
    dependencies: Arc<[RuntimeArtifactId]>,
    archive_file_name: NonEmptySharedStr,
    archive_digest: RuntimeArtifactDigest,
    platform_services: Arc<[PlatformServiceRole]>,
    native_links: Arc<[NativeLinkRequirement]>,
}

impl RuntimeArtifactComponentMetadata {
    /// Creates one physical component with canonical ownership declarations.
    pub fn try_new(
        identity: RuntimeArtifactId,
        purpose: RuntimeArtifactPurpose,
        roles: impl IntoIterator<Item = RuntimeAbiRole>,
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        archive_file_name: impl Into<Arc<str>>,
        archive_digest: RuntimeArtifactDigest,
    ) -> Result<Self, RuntimeArtifactMetadataBuildError> {
        let Some(archive_file_name) = NonEmptySharedStr::try_new(archive_file_name) else {
            return Err(RuntimeArtifactMetadataBuildError::InvalidArchiveFileName);
        };

        if !is_file_name(archive_file_name.as_str()) {
            return Err(RuntimeArtifactMetadataBuildError::InvalidArchiveFileName);
        }

        let roles = canonical_values(roles);
        let capabilities = canonical_values(capabilities);

        Ok(Self {
            identity,
            purpose,
            roles,
            capabilities,
            dependencies: Arc::from([]),
            archive_file_name,
            archive_digest,
            platform_services: Arc::from([]),
            native_links: Arc::from([]),
        })
    }

    /// Returns a component with its transitive physical support requirements.
    pub fn with_dependencies(
        mut self,
        dependencies: impl IntoIterator<Item = RuntimeArtifactId>,
    ) -> Self {
        self.dependencies = canonical_values(dependencies);

        self
    }

    /// Returns a component with the exact platform services it overrides.
    pub fn with_platform_services(
        mut self,
        platform_services: impl IntoIterator<Item = PlatformServiceRole>,
    ) -> Self {
        self.platform_services = canonical_values(platform_services);

        self
    }

    /// Returns a component with its required native libraries and frameworks.
    pub fn with_native_links(
        mut self,
        native_links: impl IntoIterator<Item = NativeLinkRequirement>,
    ) -> Self {
        self.native_links = shared_slice(native_links);

        self
    }

    /// Returns the stable component identity.
    pub const fn identity(&self) -> &RuntimeArtifactId {
        &self.identity
    }

    /// Returns the product category served by this component.
    pub const fn purpose(&self) -> RuntimeArtifactPurpose {
        self.purpose
    }

    /// Returns the runtime roles physically owned by this component.
    pub fn roles(&self) -> &[RuntimeAbiRole] {
        &self.roles
    }

    /// Returns the runtime capabilities physically owned by this component.
    pub fn capabilities(&self) -> &[RuntimeCapability] {
        &self.capabilities
    }

    /// Returns direct component dependencies in canonical identity order.
    pub fn dependencies(&self) -> &[RuntimeArtifactId] {
        &self.dependencies
    }

    /// Returns the archive file name relative to the metadata document.
    pub fn archive_file_name(&self) -> &str {
        self.archive_file_name.as_str()
    }

    /// Returns the archive content digest.
    pub const fn archive_digest(&self) -> RuntimeArtifactDigest {
        self.archive_digest
    }

    /// Returns the platform services physically overridden by this component.
    pub fn platform_services(&self) -> &[PlatformServiceRole] {
        &self.platform_services
    }

    /// Returns native libraries and frameworks needed by this component.
    pub fn native_links(&self) -> &[NativeLinkRequirement] {
        &self.native_links
    }
}

/// Immutable compiler-readable metadata published beside a runtime artifact catalog.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifactMetadata {
    contract: RuntimeContract,
    components: Arc<[RuntimeArtifactComponentMetadata]>,
}

impl RuntimeArtifactMetadata {
    /// Creates a catalog after validating exact role and capability ownership.
    pub fn try_new(
        contract: RuntimeContract,
        components: impl IntoIterator<Item = RuntimeArtifactComponentMetadata>,
    ) -> Result<Self, RuntimeArtifactMetadataBuildError> {
        let mut components: Vec<_> = components.into_iter().collect();

        components.sort_by(|left, right| {
            (left.purpose(), left.identity()).cmp(&(right.purpose(), right.identity()))
        });

        crate::component_validation::validate(&contract, &components)?;

        Ok(Self {
            contract,
            components: components.into(),
        })
    }

    /// Decodes and validates one bounded JSON metadata document.
    pub fn decode_json(bytes: &[u8]) -> Result<Self, RuntimeArtifactMetadataDecodeError> {
        if bytes.len() > MAXIMUM_METADATA_BYTES {
            return Err(RuntimeArtifactMetadataDecodeError::SizeLimitExceeded);
        }

        let wire: ArtifactWire = serde_json::from_slice(bytes)
            .map_err(|_| RuntimeArtifactMetadataDecodeError::Malformed)?;

        Self::from_wire(wire)
    }

    /// Encodes this metadata as a deterministic JSON document.
    pub fn encode_json(&self) -> Result<Vec<u8>, RuntimeArtifactMetadataEncodeError> {
        let mut bytes = serde_json::to_vec_pretty(&ArtifactWire::from_metadata(self))
            .map_err(|_| RuntimeArtifactMetadataEncodeError::Serialization)?;

        bytes.push(b'\n');

        if bytes.len() > MAXIMUM_METADATA_BYTES {
            return Err(RuntimeArtifactMetadataEncodeError::SizeLimitExceeded);
        }

        Ok(bytes)
    }

    /// Returns the published runtime compatibility contract.
    pub const fn contract(&self) -> &RuntimeContract {
        &self.contract
    }

    /// Returns physical components in canonical purpose and identity order.
    pub fn components(&self) -> &[RuntimeArtifactComponentMetadata] {
        &self.components
    }

    fn from_wire(wire: ArtifactWire) -> Result<Self, RuntimeArtifactMetadataDecodeError> {
        if wire.format != FORMAT || wire.format_version != FORMAT_VERSION {
            return Err(RuntimeArtifactMetadataDecodeError::UnsupportedFormat);
        }

        let identity = RuntimeIdentity::try_new(wire.runtime)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidRuntimeIdentity)?;

        let artifact = RuntimeArtifactId::try_new(wire.artifact)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidArtifactIdentity)?;

        let target = TargetIdentity::try_new(wire.target)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidTarget)?;

        let panic_abi = PanicAbiIdentity::try_new(wire.panic_abi)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidPanicAbi)?;

        let capabilities = wire
            .capabilities
            .iter()
            .map(|name| {
                RuntimeCapability::from_name(name)
                    .ok_or(RuntimeArtifactMetadataDecodeError::UnknownCapability)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let bindings = wire
            .roles
            .into_iter()
            .map(RoleWire::into_binding)
            .collect::<Result<Vec<_>, _>>()?;

        let contract = RuntimeContract::try_new(
            identity,
            artifact,
            wire.abi.into_version(),
            wire.frame_abi.into_versions(),
            target,
            panic_abi,
            capabilities,
            bindings,
        )
        .map_err(RuntimeArtifactMetadataDecodeError::InvalidContract)?;

        let components = wire
            .components
            .into_iter()
            .map(ComponentWire::into_metadata)
            .collect::<Result<Vec<_>, _>>()?;

        Self::try_new(contract, components)
            .map_err(RuntimeArtifactMetadataDecodeError::InvalidMetadata)
    }
}

/// A contract violation that prevents runtime artifact metadata construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeArtifactMetadataBuildError {
    /// The archive name is empty or contains a path component.
    InvalidArchiveFileName,
    /// A support-only component is not reachable from an owning component.
    UnreferencedSupportComponent(RuntimeArtifactId),
    /// More than one component has the same stable identity.
    DuplicateComponent(RuntimeArtifactId),
    /// A component depends on itself or a component absent from its product category.
    InvalidComponentDependency {
        /// Component declaring the invalid dependency.
        component: RuntimeArtifactId,
        /// Missing, cross-purpose, or self dependency.
        dependency: RuntimeArtifactId,
    },
    /// Component dependencies contain a cycle.
    ComponentDependencyCycle(RuntimeArtifactId),
    /// A component claims a role absent from the runtime contract.
    UnknownComponentRole(RuntimeAbiRole),
    /// A component claims a capability absent from the runtime contract.
    UnknownComponentCapability(RuntimeCapability),
    /// An ordinary product component claims the test-host-only entry role.
    TestRoleInProductComponent(RuntimeArtifactId),
    /// One role has no physical owner for a product category.
    MissingRoleOwner {
        /// Product category whose ownership map is incomplete.
        purpose: RuntimeArtifactPurpose,
        /// Unowned runtime role.
        role: RuntimeAbiRole,
    },
    /// One role is physically owned more than once for a product category.
    DuplicateRoleOwner {
        /// Product category whose ownership map is contradictory.
        purpose: RuntimeArtifactPurpose,
        /// Multiply owned runtime role.
        role: RuntimeAbiRole,
    },
    /// One capability has no physical owner for a product category.
    MissingCapabilityOwner {
        /// Product category whose ownership map is incomplete.
        purpose: RuntimeArtifactPurpose,
        /// Unowned runtime capability.
        capability: RuntimeCapability,
    },
    /// One capability is physically owned more than once for a product category.
    DuplicateCapabilityOwner {
        /// Product category whose ownership map is contradictory.
        purpose: RuntimeArtifactPurpose,
        /// Multiply owned runtime capability.
        capability: RuntimeCapability,
    },
    /// One platform service is overridden more than once for a product category.
    DuplicatePlatformServiceOwner {
        /// Product category whose platform overrides are contradictory.
        purpose: RuntimeArtifactPurpose,
        /// Multiply owned platform service.
        role: PlatformServiceRole,
    },
}

/// Failure to encode validated runtime artifact metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeArtifactMetadataEncodeError {
    /// Serialization of validated metadata failed.
    Serialization,
    /// The encoded document exceeds the fixed metadata resource limit.
    SizeLimitExceeded,
}

/// A malformed or incompatible runtime artifact metadata document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeArtifactMetadataDecodeError {
    /// The metadata exceeds the fixed resource limit.
    SizeLimitExceeded,
    /// The document is not valid metadata JSON.
    Malformed,
    /// The format identity or version is unsupported.
    UnsupportedFormat,
    /// The runtime implementation identity is empty.
    InvalidRuntimeIdentity,
    /// The runtime artifact identity is empty.
    InvalidArtifactIdentity,
    /// The target identity is empty.
    InvalidTarget,
    /// The panic ABI identity is empty.
    InvalidPanicAbi,
    /// A capability name is not part of the closed runtime contract.
    UnknownCapability,
    /// A role name is not part of the closed runtime contract.
    UnknownRole,
    /// A platform-service name is not part of the closed platform contract.
    UnknownPlatformService,
    /// A role symbol name is empty.
    InvalidRoleSymbol,
    /// A role implementation boundary is unknown.
    UnknownRoleImplementation,
    /// A native link requirement has an empty name.
    InvalidNativeLinkName,
    /// A native link requirement has an unknown category.
    UnknownNativeLinkKind,
    /// A component product category is unknown.
    UnknownComponentPurpose,
    /// A component identity is empty.
    InvalidComponentIdentity,
    /// The archive digest is not a 32-byte lowercase hexadecimal value.
    InvalidArchiveDigest,
    /// The runtime contract is internally inconsistent.
    InvalidContract(RuntimeContractBuildError),
    /// The archive metadata is internally inconsistent.
    InvalidMetadata(RuntimeArtifactMetadataBuildError),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArtifactWire {
    format: String,
    format_version: u16,
    runtime: String,
    artifact: String,
    abi: VersionWire,
    frame_abi: FrameAbiWire,
    target: String,
    panic_abi: String,
    capabilities: Vec<String>,
    roles: Vec<RoleWire>,
    components: Vec<ComponentWire>,
}

impl ArtifactWire {
    fn from_metadata(metadata: &RuntimeArtifactMetadata) -> Self {
        let contract = metadata.contract();

        Self {
            format: FORMAT.to_owned(),
            format_version: FORMAT_VERSION,
            runtime: contract.identity().as_str().to_owned(),
            artifact: contract.artifact().as_str().to_owned(),
            abi: VersionWire::from_version(contract.abi_version()),
            frame_abi: FrameAbiWire::from_versions(contract.frame_abi()),
            target: contract.target().as_str().to_owned(),
            panic_abi: contract.panic_abi().as_str().to_owned(),
            capabilities: contract
                .capabilities()
                .iter()
                .map(|capability| capability.as_str().to_owned())
                .collect(),
            roles: contract
                .role_bindings()
                .iter()
                .map(RoleWire::from_binding)
                .collect(),
            components: metadata
                .components()
                .iter()
                .map(ComponentWire::from_metadata)
                .collect(),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ComponentWire {
    identity: String,
    purpose: String,
    roles: Vec<String>,
    capabilities: Vec<String>,
    dependencies: Vec<String>,
    native_links: Vec<NativeLinkWire>,
    platform_services: Vec<String>,
    archive: ArchiveWire,
}

impl ComponentWire {
    fn from_metadata(metadata: &RuntimeArtifactComponentMetadata) -> Self {
        Self {
            identity: metadata.identity().as_str().to_owned(),
            purpose: metadata.purpose().as_str().to_owned(),
            roles: metadata
                .roles()
                .iter()
                .map(|role| role.as_str().to_owned())
                .collect(),
            capabilities: metadata
                .capabilities()
                .iter()
                .map(|capability| capability.as_str().to_owned())
                .collect(),
            dependencies: metadata
                .dependencies()
                .iter()
                .map(|dependency| dependency.as_str().to_owned())
                .collect(),
            native_links: metadata
                .native_links()
                .iter()
                .map(NativeLinkWire::from_requirement)
                .collect(),
            platform_services: metadata
                .platform_services()
                .iter()
                .map(|role| role.as_str().to_owned())
                .collect(),
            archive: ArchiveWire {
                file: metadata.archive_file_name().to_owned(),
                digest: metadata.archive_digest().to_hex(),
            },
        }
    }

    fn into_metadata(
        self,
    ) -> Result<RuntimeArtifactComponentMetadata, RuntimeArtifactMetadataDecodeError> {
        let identity = RuntimeArtifactId::try_new(self.identity)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidComponentIdentity)?;

        let purpose = RuntimeArtifactPurpose::from_name(&self.purpose)
            .ok_or(RuntimeArtifactMetadataDecodeError::UnknownComponentPurpose)?;

        let roles = self
            .roles
            .iter()
            .map(|role| {
                RuntimeAbiRole::from_name(role)
                    .ok_or(RuntimeArtifactMetadataDecodeError::UnknownRole)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let capabilities = self
            .capabilities
            .iter()
            .map(|capability| {
                RuntimeCapability::from_name(capability)
                    .ok_or(RuntimeArtifactMetadataDecodeError::UnknownCapability)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let native_links = self
            .native_links
            .into_iter()
            .map(NativeLinkWire::into_requirement)
            .collect::<Result<Vec<_>, _>>()?;

        let platform_services = self
            .platform_services
            .iter()
            .map(|role| {
                PlatformServiceRole::from_name(role)
                    .ok_or(RuntimeArtifactMetadataDecodeError::UnknownPlatformService)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let dependencies = self
            .dependencies
            .into_iter()
            .map(|identity| {
                RuntimeArtifactId::try_new(identity)
                    .ok_or(RuntimeArtifactMetadataDecodeError::InvalidComponentIdentity)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let digest = RuntimeArtifactDigest::from_hex(&self.archive.digest)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidArchiveDigest)?;

        RuntimeArtifactComponentMetadata::try_new(
            identity,
            purpose,
            roles,
            capabilities,
            self.archive.file,
            digest,
        )
        .map(|metadata| metadata.with_dependencies(dependencies))
        .map(|metadata| metadata.with_platform_services(platform_services))
        .map(|metadata| metadata.with_native_links(native_links))
        .map_err(RuntimeArtifactMetadataDecodeError::InvalidMetadata)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct VersionWire {
    major: u16,
    minor: u16,
}

impl VersionWire {
    const fn from_version(version: RuntimeAbiVersion) -> Self {
        Self {
            major: version.major(),
            minor: version.minor(),
        }
    }

    const fn into_version(self) -> RuntimeAbiVersion {
        RuntimeAbiVersion::new(self.major, self.minor)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FrameAbiWire {
    resume: VersionWire,
    task_broadcast: VersionWire,
    lifecycle_resolution: VersionWire,
    completion_move: VersionWire,
    destruction: VersionWire,
}

impl FrameAbiWire {
    fn from_versions(versions: ProtectedFrameAbiVersions) -> Self {
        Self {
            resume: version_for(versions, ProtectedFrameAbiOperation::Resume),
            task_broadcast: version_for(versions, ProtectedFrameAbiOperation::TaskBroadcast),
            lifecycle_resolution: version_for(
                versions,
                ProtectedFrameAbiOperation::LifecycleResolution,
            ),
            completion_move: version_for(versions, ProtectedFrameAbiOperation::CompletionMove),
            destruction: version_for(versions, ProtectedFrameAbiOperation::Destruction),
        }
    }

    const fn into_versions(self) -> ProtectedFrameAbiVersions {
        ProtectedFrameAbiVersions::new(
            self.resume.into_version(),
            self.task_broadcast.into_version(),
            self.lifecycle_resolution.into_version(),
            self.completion_move.into_version(),
            self.destruction.into_version(),
        )
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RoleWire {
    role: String,
    symbol: String,
    implementation: String,
}

impl RoleWire {
    fn from_binding(binding: &RuntimeRoleBinding) -> Self {
        Self {
            role: binding.role().as_str().to_owned(),
            symbol: binding.symbol_name().as_str().to_owned(),
            implementation: binding.implementation().as_str().to_owned(),
        }
    }

    fn into_binding(self) -> Result<RuntimeRoleBinding, RuntimeArtifactMetadataDecodeError> {
        let role = RuntimeAbiRole::from_name(&self.role)
            .ok_or(RuntimeArtifactMetadataDecodeError::UnknownRole)?;

        let symbol = BinarySymbolName::try_new(self.symbol)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidRoleSymbol)?;

        let implementation = RuntimeRoleImplementation::from_name(&self.implementation)
            .ok_or(RuntimeArtifactMetadataDecodeError::UnknownRoleImplementation)?;

        Ok(RuntimeRoleBinding::new(role, symbol, implementation))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeLinkWire {
    name: String,
    kind: String,
}

impl NativeLinkWire {
    fn from_requirement(requirement: &NativeLinkRequirement) -> Self {
        Self {
            name: requirement.name().to_owned(),
            kind: requirement.kind().as_str().to_owned(),
        }
    }

    fn into_requirement(self) -> Result<NativeLinkRequirement, RuntimeArtifactMetadataDecodeError> {
        let name = NonEmptySharedStr::try_new(self.name)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidNativeLinkName)?;

        let kind = NativeLinkKind::for_name(&self.kind)
            .ok_or(RuntimeArtifactMetadataDecodeError::UnknownNativeLinkKind)?;

        Ok(NativeLinkRequirement::new(name, kind))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ArchiveWire {
    file: String,
    digest: String,
}

const fn version_for(
    versions: ProtectedFrameAbiVersions,
    operation: ProtectedFrameAbiOperation,
) -> VersionWire {
    VersionWire::from_version(versions.operation(operation))
}

fn canonical_values<T: Ord>(values: impl IntoIterator<Item = T>) -> Arc<[T]> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn is_file_name(value: &str) -> bool {
    let path = Path::new(value);

    path.file_name().and_then(|name| name.to_str()) == Some(value)
        && path
            .parent()
            .is_some_and(|parent| parent.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bray_base::NonEmptySharedStr;
    use bray_runtime_abi::MAIN_THREAD_LANE_STARTUP_SYMBOL;
    use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
    use bray_target::TargetIdentity;

    use super::{
        MAXIMUM_METADATA_BYTES, RuntimeArtifactComponentMetadata, RuntimeArtifactDigest,
        RuntimeArtifactMetadata, RuntimeArtifactMetadataBuildError,
        RuntimeArtifactMetadataDecodeError, RuntimeArtifactMetadataEncodeError,
        RuntimeArtifactPurpose,
    };
    use crate::{
        BinarySymbolName, PanicAbiIdentity, PlatformServiceRole, ProtectedFrameAbiVersions,
        RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifact, RuntimeArtifactBuildError,
        RuntimeArtifactId, RuntimeArtifactSelectionError, RuntimeCapability, RuntimeContract,
        RuntimeIdentity, RuntimeRequirements, RuntimeRoleBinding, RuntimeRoleImplementation,
    };

    #[test]
    fn runtime_artifact_metadata_round_trips_deterministically() {
        let metadata = metadata();

        let first = metadata
            .encode_json()
            .unwrap_or_else(|_| panic!("test metadata must encode"));

        let decoded = RuntimeArtifactMetadata::decode_json(&first)
            .unwrap_or_else(|error| panic!("test metadata must decode: {error:?}"));

        let second = decoded
            .encode_json()
            .unwrap_or_else(|_| panic!("decoded metadata must encode"));

        assert_eq!(decoded, metadata);
        assert_eq!(second, first);
    }

    #[test]
    fn runtime_artifacts_require_the_published_archive_name() {
        assert!(RuntimeArtifact::try_new(metadata(), resolved_components(Path::new(""))).is_ok());

        let mut mismatched_name = resolved_components(Path::new(""));
        mismatched_name[0].1 = "other.lib".into();

        assert_eq!(
            RuntimeArtifact::try_new(metadata(), mismatched_name),
            Err(RuntimeArtifactBuildError::ArchiveFileNameMismatch)
        );
    }

    #[test]
    fn runtime_artifact_metadata_is_bounded_and_validated() {
        assert_eq!(
            RuntimeArtifactMetadata::decode_json(&vec![b' '; 64 * 1024 + 1]),
            Err(RuntimeArtifactMetadataDecodeError::SizeLimitExceeded)
        );

        assert_eq!(
            RuntimeArtifactMetadata::decode_json(b"{}"),
            Err(RuntimeArtifactMetadataDecodeError::Malformed)
        );

        let mut document = metadata()
            .encode_json()
            .unwrap_or_else(|_| panic!("test metadata must encode"));

        assert_eq!(document.pop(), Some(b'\n'));
        assert_eq!(document.pop(), Some(b'}'));

        document.extend_from_slice(br#","unknown":true}"#);

        assert_eq!(
            RuntimeArtifactMetadata::decode_json(&document),
            Err(RuntimeArtifactMetadataDecodeError::Malformed)
        );

        let encoded = metadata()
            .encode_json()
            .unwrap_or_else(|_| panic!("test metadata must encode"));

        let fixed_length = encoded.len() - "bray.runtime.reference".len();
        let fitting_identity = "x".repeat(MAXIMUM_METADATA_BYTES - fixed_length);

        let fitting = metadata_with_identity(&fitting_identity)
            .encode_json()
            .unwrap_or_else(|_| panic!("limit-sized metadata must encode"));

        assert_eq!(fitting.len(), MAXIMUM_METADATA_BYTES);

        let oversized_identity = format!("{fitting_identity}x");

        assert_eq!(
            metadata_with_identity(&oversized_identity).encode_json(),
            Err(RuntimeArtifactMetadataEncodeError::SizeLimitExceeded)
        );
    }

    #[test]
    fn runtime_selection_uses_exact_purpose_and_requirement_owners() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test runtime directory must exist: {error}"));

        let bytes = b"!<arch>\n";
        let components = resolved_components(directory.path());

        for (_, archive) in &components {
            std::fs::write(archive, bytes)
                .unwrap_or_else(|error| panic!("test runtime archive must be written: {error}"));
        }

        let digest = RuntimeArtifactDigest::new(
            bray_base::sha256_file(&components[0].1)
                .unwrap_or_else(|error| panic!("test runtime archive must hash: {error}")),
        );

        let metadata = metadata_with_digest("bray.runtime.reference", digest);

        let artifact = RuntimeArtifact::try_new(metadata, components)
            .unwrap_or_else(|error| panic!("test runtime must resolve: {error:?}"));

        let empty = requirements([], []);

        assert!(
            artifact
                .select(RuntimeArtifactPurpose::Product, &empty)
                .unwrap_or_else(|error| panic!("empty selection must succeed: {error:?}"))
                .components()
                .is_empty()
        );

        let main_thread = requirements([], [RuntimeCapability::MainThreadLane]);

        let selected = artifact
            .select(RuntimeArtifactPurpose::Product, &main_thread)
            .unwrap_or_else(|error| panic!("capability selection must succeed: {error:?}"));

        assert_eq!(
            selected.components()[0].metadata().identity().as_str(),
            "runtime.product.main_thread"
        );

        let startup = requirements([RuntimeAbiRole::MainThreadLaneStartup], []);

        let selected = artifact
            .select(RuntimeArtifactPurpose::TestRunner, &startup)
            .unwrap_or_else(|error| panic!("role selection must succeed: {error:?}"));

        assert_eq!(
            selected.components()[0].metadata().identity().as_str(),
            "runtime.test.execution"
        );

        std::fs::write(
            directory.path().join("bray_runtime_product.lib"),
            archive_with_member(b"tampered archive"),
        )
        .unwrap_or_else(|error| panic!("test runtime archive must be replaced: {error}"));

        artifact
            .select(RuntimeArtifactPurpose::Product, &main_thread)
            .unwrap_or_else(|error| {
                panic!("unselected archives must not be authenticated: {error:?}")
            });

        let archive = directory.path().join("bray_runtime_product.lib");

        let actual = RuntimeArtifactDigest::new(
            bray_base::sha256_file(&archive)
                .unwrap_or_else(|error| panic!("tampered runtime archive must hash: {error}")),
        );

        assert_eq!(
            artifact.select(RuntimeArtifactPurpose::Product, &startup),
            Err(RuntimeArtifactSelectionError::ArchiveDigestMismatch {
                component: RuntimeArtifactId::try_new("runtime.product.execution")
                    .unwrap_or_else(|| panic!("component identity must be valid")),
                path: archive,
                expected: digest,
                actual,
            })
        );
    }

    #[test]
    fn runtime_selection_preserves_missing_and_invalid_archive_details() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test runtime directory must exist: {error}"));

        let bytes = b"!<arch>\n";

        let digest = RuntimeArtifactDigest::new(
            bray_base::sha256_reader(bytes.as_slice())
                .unwrap_or_else(|error| panic!("test runtime bytes must hash: {error}")),
        );

        let components = resolved_components(directory.path());

        for (_, archive) in &components {
            std::fs::write(archive, bytes)
                .unwrap_or_else(|error| panic!("test runtime archive must be written: {error}"));
        }

        let artifact = RuntimeArtifact::try_new(
            metadata_with_digest("bray.runtime.reference", digest),
            components,
        )
        .unwrap_or_else(|error| panic!("test runtime must resolve: {error:?}"));

        let requirements = requirements([], [RuntimeCapability::MainThreadLane]);
        let archive = directory.path().join("bray_runtime_product_main.lib");

        std::fs::remove_file(&archive)
            .unwrap_or_else(|error| panic!("test runtime archive must be removed: {error}"));

        assert_eq!(
            artifact.select(RuntimeArtifactPurpose::Product, &requirements),
            Err(RuntimeArtifactSelectionError::UnreadableArchive {
                component: RuntimeArtifactId::try_new("runtime.product.main_thread")
                    .unwrap_or_else(|| panic!("component identity must be valid")),
                path: archive.clone(),
                kind: std::io::ErrorKind::NotFound,
            })
        );

        std::fs::write(&archive, b"!<arch>\ntruncated member header")
            .unwrap_or_else(|error| panic!("invalid test archive must be written: {error}"));

        assert_eq!(
            artifact.select(RuntimeArtifactPurpose::Product, &requirements),
            Err(RuntimeArtifactSelectionError::InvalidArchive {
                component: RuntimeArtifactId::try_new("runtime.product.main_thread")
                    .unwrap_or_else(|| panic!("component identity must be valid")),
                path: archive,
            })
        );
    }

    #[test]
    fn runtime_selection_rejects_malformed_member_framing() {
        let mut malformed_size = archive_with_member(&[]);
        malformed_size[8 + 48] = b'x';

        let mut truncated_member = archive_with_member(b"body");
        truncated_member.pop();

        let mut invalid_padding = archive_with_member(b"x");

        *invalid_padding
            .last_mut()
            .unwrap_or_else(|| panic!("odd archive member must have padding")) = 0;

        let mut incomplete_trailing_header = b"!<arch>\n".to_vec();
        incomplete_trailing_header.push(b'x');

        for bytes in [
            malformed_size,
            truncated_member,
            invalid_padding,
            incomplete_trailing_header,
        ] {
            assert_invalid_archive(&bytes);
        }
    }

    #[test]
    fn runtime_selection_accepts_aix_big_archive_framing() {
        assert_valid_archive(&big_archive_with_member(b"body"));
    }

    #[test]
    fn runtime_selection_accepts_supported_unix_archive_framing() {
        assert_valid_archive(&archive_with_member(b"body"));
        assert_valid_archive(&coff_archive_with_string_table());
        assert_valid_archive(&darwin_archive_with_member(b"body"));
    }

    #[test]
    fn runtime_selection_rejects_malformed_aix_big_archive_framing() {
        let mut malformed_fixed_offset = big_archive_with_member(b"body");
        malformed_fixed_offset[8] = b'x';

        let mut malformed_member_size = big_archive_with_member(b"body");
        malformed_member_size[128] = b'x';

        let mut invalid_name_terminator = big_archive_with_member(b"body");
        invalid_name_terminator[128 + 112 + 6] = 0;

        let mut truncated_member = big_archive_with_member(b"body");
        truncated_member.pop();

        let mut broken_previous_link = big_archive_with_member(b"body");
        let member_table = 128 + 112 + 6 + 2 + 4;
        broken_previous_link[member_table + 40] = b'0';

        for bytes in [
            b"<bigaf>\n".to_vec(),
            malformed_fixed_offset,
            malformed_member_size,
            invalid_name_terminator,
            truncated_member,
            broken_previous_link,
        ] {
            assert_invalid_archive(&bytes);
        }
    }

    #[test]
    fn runtime_catalogs_reject_missing_and_duplicate_capability_owners() {
        let components = metadata().components().to_vec();

        let incomplete = components
            .iter()
            .filter(|component| component.identity().as_str() != "runtime.product.main_thread")
            .cloned();

        assert_eq!(
            RuntimeArtifactMetadata::try_new(contract("bray.runtime.reference"), incomplete),
            Err(RuntimeArtifactMetadataBuildError::MissingCapabilityOwner {
                purpose: RuntimeArtifactPurpose::Product,
                capability: RuntimeCapability::MainThreadLane,
            })
        );

        let duplicate = component(
            RuntimeArtifactPurpose::Product,
            "runtime.product.cooperative_duplicate",
            "bray_runtime_product_cooperative_duplicate.lib",
            [],
            [RuntimeCapability::CooperativeExecution],
            RuntimeArtifactDigest::new([7; 32]),
        );

        assert_eq!(
            RuntimeArtifactMetadata::try_new(
                contract("bray.runtime.reference"),
                components.into_iter().chain([duplicate]),
            ),
            Err(
                RuntimeArtifactMetadataBuildError::DuplicateCapabilityOwner {
                    purpose: RuntimeArtifactPurpose::Product,
                    capability: RuntimeCapability::CooperativeExecution,
                }
            )
        );
    }

    #[test]
    fn runtime_catalogs_reject_duplicate_platform_service_overrides() {
        let metadata = metadata();

        let components = metadata
            .components()
            .iter()
            .cloned()
            .map(|component| {
                if component.purpose() == RuntimeArtifactPurpose::Product {
                    component.with_platform_services([PlatformServiceRole::StandardOutputWrite])
                } else {
                    component
                }
            });

        assert_eq!(
            RuntimeArtifactMetadata::try_new(contract("bray.runtime.reference"), components),
            Err(
                RuntimeArtifactMetadataBuildError::DuplicatePlatformServiceOwner {
                    purpose: RuntimeArtifactPurpose::Product,
                    role: PlatformServiceRole::StandardOutputWrite,
                }
            )
        );
    }

    #[test]
    fn runtime_component_dependencies_are_validated_and_selected() {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test runtime directory must exist: {error}"));

        let bytes = b"!<arch>\n";
        let digest_path = directory.path().join("digest.lib");

        std::fs::write(&digest_path, bytes)
            .unwrap_or_else(|error| panic!("test runtime archive must be written: {error}"));

        let digest = RuntimeArtifactDigest::new(
            bray_base::sha256_file(&digest_path)
                .unwrap_or_else(|error| panic!("test runtime archive must hash: {error}")),
        );

        let product_support = RuntimeArtifactId::try_new("runtime.product.support")
            .unwrap_or_else(|| panic!("component identity must be valid"));

        let test_support = RuntimeArtifactId::try_new("runtime.test.support")
            .unwrap_or_else(|| panic!("component identity must be valid"));

        let mut components = metadata_with_digest("bray.runtime.reference", digest)
            .components()
            .iter()
            .cloned()
            .map(|component| match component.identity().as_str() {
                "runtime.product.execution" => {
                    component.with_dependencies([product_support.clone()])
                }
                "runtime.product.main_thread" => {
                    component.with_dependencies([product_support.clone()])
                }
                "runtime.test.execution" => component.with_dependencies([test_support.clone()]),
                _ => component,
            })
            .collect::<Vec<_>>();

        components.extend([
            component(
                RuntimeArtifactPurpose::Product,
                product_support.as_str(),
                "bray_runtime_product_support.lib",
                [],
                [],
                digest,
            ),
            component(
                RuntimeArtifactPurpose::TestRunner,
                test_support.as_str(),
                "bray_runtime_test_support.lib",
                [],
                [],
                digest,
            ),
        ]);

        let metadata =
            RuntimeArtifactMetadata::try_new(contract("bray.runtime.reference"), components)
                .unwrap_or_else(|error| {
                    panic!("dependent runtime metadata must validate: {error:?}")
                });

        let mut resolved = Vec::new();

        for component in metadata.components() {
            let archive = directory.path().join(component.archive_file_name());

            std::fs::write(&archive, bytes)
                .unwrap_or_else(|error| panic!("test runtime archive must be written: {error}"));

            resolved.push((component.identity().clone(), archive));
        }

        let artifact = RuntimeArtifact::try_new(metadata, resolved)
            .unwrap_or_else(|error| panic!("test runtime must resolve: {error:?}"));

        let selected = artifact
            .select(
                RuntimeArtifactPurpose::Product,
                &requirements(
                    [RuntimeAbiRole::MainThreadLaneStartup],
                    [RuntimeCapability::MainThreadLane],
                ),
            )
            .unwrap_or_else(|error| panic!("dependent selection must succeed: {error:?}"));

        let identities = selected
            .components()
            .iter()
            .map(|component| component.metadata().identity().as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            identities,
            [
                "runtime.product.execution",
                "runtime.product.main_thread",
                "runtime.product.support",
            ]
        );
    }

    fn metadata() -> RuntimeArtifactMetadata {
        metadata_with_identity("bray.runtime.reference")
    }

    fn metadata_with_identity(identity: &str) -> RuntimeArtifactMetadata {
        metadata_with_digest(identity, RuntimeArtifactDigest::new([7; 32]))
    }

    fn metadata_with_digest(
        identity: &str,
        digest: RuntimeArtifactDigest,
    ) -> RuntimeArtifactMetadata {
        let native_link = NativeLinkRequirement::new(
            NonEmptySharedStr::try_new("userenv")
                .unwrap_or_else(|| panic!("test native library name must be valid")),
            NativeLinkKind::System,
        );

        RuntimeArtifactMetadata::try_new(
            contract(identity),
            [
                component(
                    RuntimeArtifactPurpose::Product,
                    "runtime.product.execution",
                    "bray_runtime_product.lib",
                    [RuntimeAbiRole::MainThreadLaneStartup],
                    [RuntimeCapability::CooperativeExecution],
                    digest,
                )
                .with_native_links([native_link.clone()]),
                component(
                    RuntimeArtifactPurpose::Product,
                    "runtime.product.main_thread",
                    "bray_runtime_product_main.lib",
                    [],
                    [RuntimeCapability::MainThreadLane],
                    digest,
                ),
                component(
                    RuntimeArtifactPurpose::TestRunner,
                    "runtime.test.execution",
                    "bray_runtime_test.lib",
                    [RuntimeAbiRole::MainThreadLaneStartup],
                    [RuntimeCapability::CooperativeExecution],
                    digest,
                )
                .with_platform_services([PlatformServiceRole::StandardOutputWrite])
                .with_native_links([native_link]),
                component(
                    RuntimeArtifactPurpose::TestRunner,
                    "runtime.test.main_thread",
                    "bray_runtime_test_main.lib",
                    [],
                    [RuntimeCapability::MainThreadLane],
                    digest,
                ),
            ],
        )
        .unwrap_or_else(|error| panic!("test metadata must be valid: {error:?}"))
    }

    fn component<const R: usize, const C: usize>(
        purpose: RuntimeArtifactPurpose,
        identity: &str,
        archive: &str,
        roles: [RuntimeAbiRole; R],
        capabilities: [RuntimeCapability; C],
        digest: RuntimeArtifactDigest,
    ) -> RuntimeArtifactComponentMetadata {
        RuntimeArtifactComponentMetadata::try_new(
            RuntimeArtifactId::try_new(identity)
                .unwrap_or_else(|| panic!("component identity must be valid")),
            purpose,
            roles,
            capabilities,
            archive,
            digest,
        )
        .unwrap_or_else(|error| panic!("test component must be valid: {error:?}"))
    }

    fn resolved_components(directory: &Path) -> Vec<(RuntimeArtifactId, std::path::PathBuf)> {
        [
            ("runtime.product.execution", "bray_runtime_product.lib"),
            (
                "runtime.product.main_thread",
                "bray_runtime_product_main.lib",
            ),
            ("runtime.test.execution", "bray_runtime_test.lib"),
            ("runtime.test.main_thread", "bray_runtime_test_main.lib"),
        ]
        .into_iter()
        .map(|(identity, archive)| {
            (
                RuntimeArtifactId::try_new(identity)
                    .unwrap_or_else(|| panic!("component identity must be valid")),
                directory.join(archive),
            )
        })
        .collect()
    }

    fn archive_with_member(contents: &[u8]) -> Vec<u8> {
        unix_archive_member("member/", contents)
    }

    fn coff_archive_with_string_table() -> Vec<u8> {
        unix_archive_member("//", b"long_name\0")
    }

    fn darwin_archive_with_member(contents: &[u8]) -> Vec<u8> {
        let name = b"member";
        let name_padding = 6;
        let member_padding = (8 - contents.len() % 8) % 8;
        let encoded_name_length = name.len() + name_padding;
        let mut member = Vec::new();

        member.extend_from_slice(name);
        member.resize(encoded_name_length, 0);
        member.extend_from_slice(contents);
        member.resize(member.len() + member_padding, 0);

        unix_archive_member(&format!("#1/{encoded_name_length}"), &member)
    }

    fn unix_archive_member(name: &str, contents: &[u8]) -> Vec<u8> {
        let header = format!(
            "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
            name,
            0,
            0,
            0,
            "100644",
            contents.len()
        );

        assert_eq!(header.len(), 60);

        let mut archive = b"!<arch>\n".to_vec();

        archive.extend_from_slice(header.as_bytes());
        archive.extend_from_slice(contents);

        if contents.len() % 2 == 1 {
            archive.push(b'\n');
        }

        archive
    }

    fn big_archive_with_member(contents: &[u8]) -> Vec<u8> {
        const FIXED_HEADER_LENGTH: usize = 128;

        let name = b"member";
        let member_length = 112 + name.len() + 2 + contents.len() + contents.len() % 2;
        let member_table_offset = FIXED_HEADER_LENGTH + member_length;

        let member_table_contents = [
            fixed_width_decimal(1, 20),
            fixed_width_decimal(FIXED_HEADER_LENGTH, 20),
            b"member\0".to_vec(),
        ]
        .concat();

        let mut archive = b"<bigaf>\n".to_vec();

        for offset in [
            member_table_offset,
            0,
            0,
            FIXED_HEADER_LENGTH,
            FIXED_HEADER_LENGTH,
            0,
        ] {
            archive.extend_from_slice(&fixed_width_decimal(offset, 20));
        }

        archive.extend_from_slice(&big_archive_member(
            name,
            contents,
            member_table_offset,
            0,
        ));

        archive.extend_from_slice(&big_archive_member(
            b"",
            &member_table_contents,
            0,
            FIXED_HEADER_LENGTH,
        ));

        archive
    }

    fn big_archive_member(
        name: &[u8],
        contents: &[u8],
        next: usize,
        previous: usize,
    ) -> Vec<u8> {
        let mut member = Vec::new();

        for (value, width) in [
            (contents.len(), 20),
            (next, 20),
            (previous, 20),
            (0, 12),
            (0, 12),
            (0, 12),
        ] {
            member.extend_from_slice(&fixed_width_decimal(value, width));
        }

        member.extend_from_slice(format!("{:<12}", "100644").as_bytes());
        member.extend_from_slice(&fixed_width_decimal(name.len(), 4));
        member.extend_from_slice(name);

        if name.len() % 2 == 1 {
            member.push(0);
        }

        member.extend_from_slice(b"`\n");
        member.extend_from_slice(contents);

        if contents.len() % 2 == 1 {
            member.push(0);
        }

        member
    }

    fn fixed_width_decimal(value: usize, width: usize) -> Vec<u8> {
        format!("{value:<width$}").into_bytes()
    }

    fn assert_valid_archive(bytes: &[u8]) {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test runtime directory must exist: {error}"));

        let digest = RuntimeArtifactDigest::new(
            bray_base::sha256_reader(bytes)
                .unwrap_or_else(|error| panic!("test runtime bytes must hash: {error}")),
        );

        let components = resolved_components(directory.path());

        for (_, archive) in &components {
            std::fs::write(archive, bytes)
                .unwrap_or_else(|error| panic!("test runtime archive must be written: {error}"));
        }

        let artifact = RuntimeArtifact::try_new(
            metadata_with_digest("bray.runtime.reference", digest),
            components,
        )
        .unwrap_or_else(|error| panic!("test runtime must resolve: {error:?}"));

        assert!(
            artifact
                .select(
                    RuntimeArtifactPurpose::Product,
                    &requirements([], [RuntimeCapability::MainThreadLane]),
                )
                .is_ok()
        );
    }

    fn assert_invalid_archive(bytes: &[u8]) {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("test runtime directory must exist: {error}"));

        let digest = RuntimeArtifactDigest::new(
            bray_base::sha256_reader(bytes)
                .unwrap_or_else(|error| panic!("test runtime bytes must hash: {error}")),
        );

        let components = resolved_components(directory.path());

        for (_, archive) in &components {
            std::fs::write(archive, bytes)
                .unwrap_or_else(|error| panic!("test runtime archive must be written: {error}"));
        }

        let artifact = RuntimeArtifact::try_new(
            metadata_with_digest("bray.runtime.reference", digest),
            components,
        )
        .unwrap_or_else(|error| panic!("test runtime must resolve: {error:?}"));

        assert!(matches!(
            artifact.select(
                RuntimeArtifactPurpose::Product,
                &requirements([], [RuntimeCapability::MainThreadLane]),
            ),
            Err(RuntimeArtifactSelectionError::InvalidArchive { .. })
        ));
    }

    fn contract(identity: &str) -> RuntimeContract {
        RuntimeContract::try_new(
            RuntimeIdentity::try_new(identity)
                .unwrap_or_else(|| panic!("runtime identity must be valid")),
            RuntimeArtifactId::try_new("bray.runtime.reference.windows.x86_64")
                .unwrap_or_else(|| panic!("artifact identity must be valid")),
            RuntimeAbiVersion::new(1, 0),
            ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(1, 0)),
            TargetIdentity::try_new("x86_64-pc-windows-msvc")
                .unwrap_or_else(|| panic!("target must be valid")),
            PanicAbiIdentity::try_new("bray.panic.unwind")
                .unwrap_or_else(|| panic!("panic ABI must be valid")),
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ],
            [RuntimeRoleBinding::new(
                RuntimeAbiRole::MainThreadLaneStartup,
                BinarySymbolName::try_new(MAIN_THREAD_LANE_STARTUP_SYMBOL)
                    .unwrap_or_else(|| panic!("runtime symbol must be valid")),
                RuntimeRoleImplementation::BrayRuntime,
            )],
        )
        .unwrap_or_else(|error| panic!("test runtime contract must be valid: {error:?}"))
    }

    fn requirements<const R: usize, const C: usize>(
        roles: [RuntimeAbiRole; R],
        capabilities: [RuntimeCapability; C],
    ) -> RuntimeRequirements {
        RuntimeRequirements::new(
            None,
            RuntimeAbiVersion::new(1, 0),
            None,
            TargetIdentity::try_new("x86_64-pc-windows-msvc")
                .unwrap_or_else(|| panic!("target must be valid")),
            PanicAbiIdentity::try_new("bray.panic.unwind")
                .unwrap_or_else(|| panic!("panic ABI must be valid")),
            roles,
            capabilities,
            [],
        )
    }
}
