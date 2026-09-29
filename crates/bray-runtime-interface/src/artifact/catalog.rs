use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
pub use bray_native_artifact::NativeContentDigest as RuntimeArtifactDigest;
use bray_target::TargetIdentity;
use serde::{Deserialize, Serialize};

use crate::{
    BinarySymbolName, PanicAbiIdentity, PlatformServiceRole, ProtectedFrameAbiOperation,
    ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactId,
    RuntimeCapability, RuntimeContract, RuntimeContractBuildError, RuntimeIdentity,
    RuntimeRoleBinding, RuntimeRoleImplementation,
};

const FORMAT: &str = "bray_native_runtime";
const FORMAT_VERSION: u16 = 1;
const MAXIMUM_METADATA_BYTES: usize = 64 * 1024;

/// Product category used to select one non-overlapping runtime implementation surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeArtifactPurpose {
    /// Ordinary executable products.
    Product,
    /// Test executables with the native test-host protocol.
    TestRunner,
}

/// Authenticated native unit index for one runtime product category.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeNativeIndexMetadata {
    purpose: RuntimeArtifactPurpose,
    file_name: NonEmptySharedStr,
    digest: RuntimeArtifactDigest,
}

impl RuntimeNativeIndexMetadata {
    /// Creates one index reference beneath the runtime artifact directory.
    pub fn try_new(
        purpose: RuntimeArtifactPurpose,
        file_name: impl Into<Arc<str>>,
        digest: RuntimeArtifactDigest,
    ) -> Result<Self, RuntimeArtifactMetadataBuildError> {
        let file_name = NonEmptySharedStr::try_new(file_name)
            .ok_or(RuntimeArtifactMetadataBuildError::InvalidNativeIndexFileName)?;

        if !is_file_name(file_name.as_str()) {
            return Err(RuntimeArtifactMetadataBuildError::InvalidNativeIndexFileName);
        }

        Ok(Self { purpose, file_name, digest })
    }

    /// Returns the runtime product category covered by this index.
    pub const fn purpose(&self) -> RuntimeArtifactPurpose { self.purpose }

    /// Returns the index file name beneath the artifact directory.
    pub fn file_name(&self) -> &str { self.file_name.as_str() }

    /// Returns the digest of the exact encoded index bytes.
    pub const fn digest(&self) -> RuntimeArtifactDigest { self.digest }
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

/// One semantic runtime provider and the roles it owns for one product category.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifactComponentMetadata {
    identity: RuntimeArtifactId,
    purpose: RuntimeArtifactPurpose,
    roles: Arc<[RuntimeAbiRole]>,
    capabilities: Arc<[RuntimeCapability]>,
    platform_services: Arc<[PlatformServiceRole]>,
}

impl RuntimeArtifactComponentMetadata {
    /// Creates one runtime provider with canonical ownership declarations.
    pub fn try_new(
        identity: RuntimeArtifactId,
        purpose: RuntimeArtifactPurpose,
        roles: impl IntoIterator<Item = RuntimeAbiRole>,
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
    ) -> Result<Self, RuntimeArtifactMetadataBuildError> {
        let roles = canonical_values(roles);
        let capabilities = canonical_values(capabilities);

        Ok(Self {
            identity,
            purpose,
            roles,
            capabilities,
            platform_services: Arc::from([]),
        })
    }

    /// Returns a component with the exact platform services it overrides.
    pub fn with_platform_services(
        mut self,
        platform_services: impl IntoIterator<Item = PlatformServiceRole>,
    ) -> Self {
        self.platform_services = canonical_values(platform_services);

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

    /// Returns the runtime roles assigned to this provider.
    pub fn roles(&self) -> &[RuntimeAbiRole] {
        &self.roles
    }

    /// Returns the runtime capabilities assigned to this provider.
    pub fn capabilities(&self) -> &[RuntimeCapability] {
        &self.capabilities
    }

    /// Returns the platform services overridden by this provider.
    pub fn platform_services(&self) -> &[PlatformServiceRole] {
        &self.platform_services
    }

}

/// Immutable compiler-readable metadata published beside a runtime artifact catalog.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifactMetadata {
    contract: RuntimeContract,
    components: Arc<[RuntimeArtifactComponentMetadata]>,
    native_indexes: Arc<[RuntimeNativeIndexMetadata]>,
}

impl RuntimeArtifactMetadata {
    /// Creates a catalog after validating exact role and capability ownership.
    pub fn try_new(
        contract: RuntimeContract,
        components: impl IntoIterator<Item = RuntimeArtifactComponentMetadata>,
        native_indexes: impl IntoIterator<Item = RuntimeNativeIndexMetadata>,
    ) -> Result<Self, RuntimeArtifactMetadataBuildError> {
        let mut components: Vec<_> = components.into_iter().collect();

        components.sort_by(|left, right| {
            (left.purpose(), left.identity()).cmp(&(right.purpose(), right.identity()))
        });

        crate::component_validation::validate(&contract, &components)?;
        let mut indexes = native_indexes.into_iter().collect::<Vec<_>>();
        indexes.sort_by_key(RuntimeNativeIndexMetadata::purpose);

        if indexes.iter().map(RuntimeNativeIndexMetadata::purpose)
            .ne(RuntimeArtifactPurpose::ALL)
        {
            return Err(RuntimeArtifactMetadataBuildError::InvalidNativeIndexes);
        }

        Ok(Self { contract, components: components.into(), native_indexes: indexes.into() })
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

    /// Returns semantic providers in canonical purpose and identity order.
    pub fn components(&self) -> &[RuntimeArtifactComponentMetadata] {
        &self.components
    }

    /// Returns the canonical per-purpose native index references.
    pub fn native_indexes(&self) -> &[RuntimeNativeIndexMetadata] {
        &self.native_indexes
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

        let indexes = wire.native_indexes.into_iter()
            .map(NativeIndexWire::into_metadata)
            .collect::<Result<Vec<_>, _>>()?;

        Self::try_new(contract, components, indexes)
            .map_err(RuntimeArtifactMetadataDecodeError::InvalidMetadata)
    }
}

/// A contract violation that prevents runtime artifact metadata construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuntimeArtifactMetadataBuildError {
    /// A native index name is empty or contains a path component.
    InvalidNativeIndexFileName,
    /// Exactly one native index is required for each runtime purpose.
    InvalidNativeIndexes,
    /// More than one component has the same stable identity.
    DuplicateComponent(RuntimeArtifactId),
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
    /// A native index digest is not a lowercase SHA-256 value.
    InvalidNativeIndexDigest,
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
    /// A component product category is unknown.
    UnknownComponentPurpose,
    /// A component identity is empty.
    InvalidComponentIdentity,
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
    native_indexes: Vec<NativeIndexWire>,
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
            native_indexes: metadata.native_indexes()
                .iter()
                .map(NativeIndexWire::from_metadata)
                .collect(),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeIndexWire {
    purpose: String,
    file: String,
    digest: String,
}

impl NativeIndexWire {
    fn from_metadata(metadata: &RuntimeNativeIndexMetadata) -> Self {
        Self {
            purpose: metadata.purpose().as_str().to_owned(),
            file: metadata.file_name().to_owned(),
            digest: metadata.digest().to_hex(),
        }
    }

    fn into_metadata(self) -> Result<RuntimeNativeIndexMetadata, RuntimeArtifactMetadataDecodeError> {
        let purpose = RuntimeArtifactPurpose::from_name(&self.purpose)
            .ok_or(RuntimeArtifactMetadataDecodeError::UnknownComponentPurpose)?;

        let digest = RuntimeArtifactDigest::from_hex(&self.digest)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidNativeIndexDigest)?;

        RuntimeNativeIndexMetadata::try_new(purpose, self.file, digest)
            .map_err(RuntimeArtifactMetadataDecodeError::InvalidMetadata)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ComponentWire {
    identity: String,
    purpose: String,
    roles: Vec<String>,
    capabilities: Vec<String>,
    platform_services: Vec<String>,
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
            platform_services: metadata
                .platform_services()
                .iter()
                .map(|role| role.as_str().to_owned())
                .collect(),
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

        let platform_services = self
            .platform_services
            .iter()
            .map(|role| {
                PlatformServiceRole::from_name(role)
                    .ok_or(RuntimeArtifactMetadataDecodeError::UnknownPlatformService)
            })
            .collect::<Result<Vec<_>, _>>()?;

        RuntimeArtifactComponentMetadata::try_new(
            identity,
            purpose,
            roles,
            capabilities,
        )
        .map(|metadata| metadata.with_platform_services(platform_services))
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
    use bray_base::NonEmptySharedStr;
    use bray_native_artifact::{NativeArtifactIndex, NativeDefinition, NativeDefinitionSelection, NativeUnit, NativeUnitKind, NativeUnitSummary};
    use bray_runtime_abi::symbols::MAIN_THREAD_LANE_STARTUP_SYMBOL;
    use bray_symbols::NativeSymbolContract;
    use bray_target::{NativeTarget, TargetIdentity};

    use super::{
        MAXIMUM_METADATA_BYTES, RuntimeArtifactComponentMetadata, RuntimeArtifactDigest, RuntimeNativeIndexMetadata,
        RuntimeArtifactMetadata, RuntimeArtifactMetadataBuildError,
        RuntimeArtifactMetadataDecodeError, RuntimeArtifactMetadataEncodeError,
        RuntimeArtifactPurpose,
    };
    use crate::{
        BinarySymbolName, PanicAbiIdentity, PlatformServiceRole, ProtectedFrameAbiVersions,
        RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifact,
        RuntimeArtifactId, RuntimeCapability, RuntimeContract,
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
    fn runtime_artifacts_require_published_native_indexes() {
        let metadata = metadata();
        assert_eq!(metadata.native_indexes().len(), 2);

        assert_eq!(
            RuntimeArtifactMetadata::try_new(metadata.contract().clone(), metadata.components().to_vec(), []),
            Err(RuntimeArtifactMetadataBuildError::InvalidNativeIndexes),
        );

        let mut document: serde_json::Value = serde_json::from_slice(&metadata.encode_json().unwrap())
            .expect("test metadata must parse");

        document.as_object_mut().expect("metadata must be an object").remove("native_indexes");

        assert_eq!(
            RuntimeArtifactMetadata::decode_json(&serde_json::to_vec(&document).unwrap()),
            Err(RuntimeArtifactMetadataDecodeError::Malformed),
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

        let index = native_index(directory.path());

        let artifact = RuntimeArtifact::try_new(metadata(), directory.path().to_path_buf(), [index.clone(), index])
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
            selected.components()[0].identity().as_str(),
            "runtime.product.main_thread"
        );

        let startup = requirements([RuntimeAbiRole::MainThreadLaneStartup], []);

        let selected = artifact
            .select(RuntimeArtifactPurpose::TestRunner, &startup)
            .unwrap_or_else(|error| panic!("role selection must succeed: {error:?}"));

        assert_eq!(
            selected.components()[0].identity().as_str(),
            "runtime.test.execution"
        );

    }

    #[test]
    fn runtime_catalogs_reject_missing_and_duplicate_capability_owners() {
        let components = metadata().components().to_vec();

        let incomplete = components
            .iter()
            .filter(|component| component.identity().as_str() != "runtime.product.main_thread")
            .cloned();

        assert_eq!(
            RuntimeArtifactMetadata::try_new(contract("bray.runtime.reference"), incomplete, test_index_refs()),
            Err(RuntimeArtifactMetadataBuildError::MissingCapabilityOwner {
                purpose: RuntimeArtifactPurpose::Product,
                capability: RuntimeCapability::MainThreadLane,
            })
        );

        let duplicate = component(
            RuntimeArtifactPurpose::Product,
            "runtime.product.cooperative_duplicate",
            [],
            [RuntimeCapability::CooperativeExecution],
        );

        assert_eq!(
            RuntimeArtifactMetadata::try_new(
                contract("bray.runtime.reference"),
                components.into_iter().chain([duplicate]),
                test_index_refs(),
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

        let components = metadata.components().iter().cloned().map(|component| {
            if component.purpose() == RuntimeArtifactPurpose::Product {
                component.with_platform_services([PlatformServiceRole::StandardOutputWrite])
            } else {
                component
            }
        });

        assert_eq!(
            RuntimeArtifactMetadata::try_new(contract("bray.runtime.reference"), components, test_index_refs()),
            Err(
                RuntimeArtifactMetadataBuildError::DuplicatePlatformServiceOwner {
                    purpose: RuntimeArtifactPurpose::Product,
                    role: PlatformServiceRole::StandardOutputWrite,
                }
            )
        );
    }

    fn metadata() -> RuntimeArtifactMetadata {
        metadata_with_identity("bray.runtime.reference")
    }

    fn metadata_with_identity(identity: &str) -> RuntimeArtifactMetadata {
        RuntimeArtifactMetadata::try_new(
            contract(identity),
            [
                component(
                    RuntimeArtifactPurpose::Product,
                    "runtime.product.execution",
                    [RuntimeAbiRole::MainThreadLaneStartup],
                    [RuntimeCapability::CooperativeExecution],
                ),
                component(
                    RuntimeArtifactPurpose::Product,
                    "runtime.product.main_thread",
                    [],
                    [RuntimeCapability::MainThreadLane],
                ),
                component(
                    RuntimeArtifactPurpose::TestRunner,
                    "runtime.test.execution",
                    [RuntimeAbiRole::MainThreadLaneStartup],
                    [RuntimeCapability::CooperativeExecution],
                )
                .with_platform_services([PlatformServiceRole::StandardOutputWrite]),
                component(
                    RuntimeArtifactPurpose::TestRunner,
                    "runtime.test.main_thread",
                    [],
                    [RuntimeCapability::MainThreadLane],
                ),
            ],
            test_index_refs(),
        )
        .unwrap_or_else(|error| panic!("test metadata must be valid: {error:?}"))
    }

    fn test_index_refs() -> [RuntimeNativeIndexMetadata; 2] {
        RuntimeArtifactPurpose::ALL.map(|purpose| {
            RuntimeNativeIndexMetadata::try_new(purpose, format!("{}.json", purpose.as_str()), RuntimeArtifactDigest::new([7; 32]))
                .unwrap_or_else(|error| panic!("test native index reference must be valid: {error:?}"))
        })
    }

    fn component<const R: usize, const C: usize>(
        purpose: RuntimeArtifactPurpose,
        identity: &str,
        roles: [RuntimeAbiRole; R],
        capabilities: [RuntimeCapability; C],
    ) -> RuntimeArtifactComponentMetadata {
        RuntimeArtifactComponentMetadata::try_new(
            RuntimeArtifactId::try_new(identity)
                .unwrap_or_else(|| panic!("component identity must be valid")),
            purpose,
            roles,
            capabilities,
        )
        .unwrap_or_else(|error| panic!("test component must be valid: {error:?}"))
    }

    fn native_index(directory: &std::path::Path) -> bray_native_artifact::ValidatedNativeArtifact {
        let bytes = b"test runtime native unit";
        let digest = RuntimeArtifactDigest::new(bray_base::sha256_reader(&bytes[..]).expect("in-memory bytes must hash"));
        let symbol = NativeSymbolContract::required_name(NonEmptySharedStr::try_new(MAIN_THREAD_LANE_STARTUP_SYMBOL).expect("startup symbol must be nonempty"));

        let unit = NativeUnit::new(digest, NativeUnitKind::Object, NativeUnitSummary::Exact {
            definitions: [NativeDefinition::new(symbol, NativeDefinitionSelection::Ordinary)].into(),
            references: [].into(), roots: [].into(),
        }, [], []);

        let target = NativeTarget::X86_64WindowsMsvc;
        let producer = RuntimeArtifactDigest::new([9; 32]);

        let index = NativeArtifactIndex::try_new(target, producer, [unit], [])
            .unwrap_or_else(|error| panic!("test index must be valid: {error:?}"));

        let encoded = index.encode().expect("test index must encode");
        let index_digest = RuntimeArtifactDigest::new(bray_base::sha256_reader(&encoded[..]).expect("in-memory index must hash"));
        let native = directory.join("native");
        std::fs::create_dir_all(&native).expect("test native directory must exist");
        std::fs::write(native.join(NativeUnitKind::Object.file_name(digest, target)), bytes).expect("test native payload must exist");

        NativeArtifactIndex::import(&encoded, index_digest, target, producer, &native)
            .unwrap_or_else(|error| panic!("test index must import: {error:?}"))
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
