use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_target::TargetIdentity;
use serde::{Deserialize, Serialize};

use crate::{
    BinarySymbolName, PanicAbiIdentity, ProtectedFrameAbiOperation,
    ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion,
    RuntimeArtifactId, RuntimeCapability, RuntimeContract,
    RuntimeContractBuildError, RuntimeIdentity, RuntimeRoleBinding,
    RuntimeRoleImplementation,
};

const FORMAT: &str = "bray_native_runtime";
const FORMAT_VERSION: u16 = 1;
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
        let mut output = String::with_capacity(64);

        for byte in self.0 {
            use std::fmt::Write as _;

            let _ = write!(output, "{byte:02x}");
        }

        output
    }

    fn from_hex(value: &str) -> Option<Self> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }

        let mut bytes = [0; 32];

        for (index, output) in bytes.iter_mut().enumerate() {
            let offset = index * 2;
            let digits = value.get(offset..offset + 2)?;

            *output = u8::from_str_radix(digits, 16).ok()?;
        }

        Some(Self(bytes))
    }
}

/// Immutable compiler-readable metadata published beside one native runtime archive.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifactMetadata {
    contract: RuntimeContract,
    archive_file_name: NonEmptySharedStr,
    archive_digest: RuntimeArtifactDigest,
}

impl RuntimeArtifactMetadata {
    /// Creates metadata for one archive file and selected runtime contract.
    pub fn try_new(
        contract: RuntimeContract,
        archive_file_name: impl Into<Arc<str>>,
        archive_digest: RuntimeArtifactDigest,
    ) -> Result<Self, RuntimeArtifactMetadataBuildError> {
        let Some(archive_file_name) =
            NonEmptySharedStr::try_new(archive_file_name)
        else {
            return Err(RuntimeArtifactMetadataBuildError::InvalidArchiveFileName);
        };

        if !is_file_name(archive_file_name.as_str()) {
            return Err(RuntimeArtifactMetadataBuildError::InvalidArchiveFileName);
        }

        Ok(Self {
            contract,
            archive_file_name,
            archive_digest,
        })
    }

    /// Decodes and validates one bounded JSON metadata document.
    pub fn decode_json(
        bytes: &[u8],
    ) -> Result<Self, RuntimeArtifactMetadataDecodeError> {
        if bytes.len() > MAXIMUM_METADATA_BYTES {
            return Err(RuntimeArtifactMetadataDecodeError::SizeLimitExceeded);
        }

        let wire: ArtifactWire = serde_json::from_slice(bytes)
            .map_err(|_| RuntimeArtifactMetadataDecodeError::Malformed)?;

        Self::from_wire(wire)
    }

    /// Encodes this metadata as a deterministic JSON document.
    pub fn encode_json(
        &self,
    ) -> Result<Vec<u8>, RuntimeArtifactMetadataEncodeError> {
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

    /// Returns the archive file name relative to the metadata document.
    pub fn archive_file_name(&self) -> &str {
        self.archive_file_name.as_str()
    }

    /// Returns the archive content digest.
    pub const fn archive_digest(&self) -> RuntimeArtifactDigest {
        self.archive_digest
    }

    fn from_wire(
        wire: ArtifactWire,
    ) -> Result<Self, RuntimeArtifactMetadataDecodeError> {
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
                RuntimeCapability::from_name(name).ok_or(
                    RuntimeArtifactMetadataDecodeError::UnknownCapability,
                )
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

        let digest = RuntimeArtifactDigest::from_hex(&wire.archive.digest)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidArchiveDigest)?;

        Self::try_new(contract, wire.archive.file, digest)
            .map_err(RuntimeArtifactMetadataDecodeError::InvalidMetadata)
    }
}

/// Resolved runtime archive selected for one compiler invocation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeArtifact {
    metadata: RuntimeArtifactMetadata,
    archive: PathBuf,
}

impl RuntimeArtifact {
    /// Resolves metadata to an exact archive path with the published file name.
    pub fn try_new(
        metadata: RuntimeArtifactMetadata,
        archive: impl Into<PathBuf>,
        archive_digest: RuntimeArtifactDigest,
    ) -> Result<Self, RuntimeArtifactBuildError> {
        let archive = archive.into();

        if archive.file_name().and_then(|name| name.to_str())
            != Some(metadata.archive_file_name())
        {
            return Err(RuntimeArtifactBuildError::ArchiveFileNameMismatch);
        }

        if archive_digest != metadata.archive_digest() {
            return Err(RuntimeArtifactBuildError::ArchiveDigestMismatch);
        }

        Ok(Self { metadata, archive })
    }

    /// Returns the immutable artifact metadata.
    pub const fn metadata(&self) -> &RuntimeArtifactMetadata {
        &self.metadata
    }

    /// Returns the selected runtime contract.
    pub const fn contract(&self) -> &RuntimeContract {
        self.metadata.contract()
    }

    /// Returns the exact native archive path.
    pub fn archive(&self) -> &Path {
        &self.archive
    }
}

/// A contract violation that prevents runtime artifact metadata construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeArtifactMetadataBuildError {
    /// The archive name is empty or contains a path component.
    InvalidArchiveFileName,
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    /// A role symbol name is empty.
    InvalidRoleSymbol,
    /// A role implementation boundary is unknown.
    UnknownRoleImplementation,
    /// The archive digest is not a 32-byte lowercase hexadecimal value.
    InvalidArchiveDigest,
    /// The runtime contract is internally inconsistent.
    InvalidContract(RuntimeContractBuildError),
    /// The archive metadata is internally inconsistent.
    InvalidMetadata(RuntimeArtifactMetadataBuildError),
}

/// A contract violation that prevents resolving metadata to an archive path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeArtifactBuildError {
    /// The selected path does not end with the published archive file name.
    ArchiveFileNameMismatch,
    /// The selected archive content does not match the published digest.
    ArchiveDigestMismatch,
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
    archive: ArchiveWire,
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
            archive: ArchiveWire {
                file: metadata.archive_file_name().to_owned(),
                digest: metadata.archive_digest().to_hex(),
            },
        }
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
            task_broadcast: version_for(
                versions,
                ProtectedFrameAbiOperation::TaskBroadcast,
            ),
            lifecycle_resolution: version_for(
                versions,
                ProtectedFrameAbiOperation::LifecycleResolution,
            ),
            completion_move: version_for(
                versions,
                ProtectedFrameAbiOperation::CompletionMove,
            ),
            destruction: version_for(
                versions,
                ProtectedFrameAbiOperation::Destruction,
            ),
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

    fn into_binding(
        self,
    ) -> Result<RuntimeRoleBinding, RuntimeArtifactMetadataDecodeError> {
        let role = RuntimeAbiRole::from_name(&self.role)
            .ok_or(RuntimeArtifactMetadataDecodeError::UnknownRole)?;

        let symbol = BinarySymbolName::try_new(self.symbol)
            .ok_or(RuntimeArtifactMetadataDecodeError::InvalidRoleSymbol)?;

        let implementation =
            RuntimeRoleImplementation::from_name(&self.implementation).ok_or(
                RuntimeArtifactMetadataDecodeError::UnknownRoleImplementation,
            )?;

        Ok(RuntimeRoleBinding::new(role, symbol, implementation))
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

fn is_file_name(value: &str) -> bool {
    let path = Path::new(value);

    path.file_name().and_then(|name| name.to_str()) == Some(value)
        && path.parent().is_some_and(|parent| parent.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use bray_target::TargetIdentity;

    use super::{
        MAXIMUM_METADATA_BYTES, RuntimeArtifact, RuntimeArtifactBuildError,
        RuntimeArtifactDigest, RuntimeArtifactMetadata,
        RuntimeArtifactMetadataDecodeError,
        RuntimeArtifactMetadataEncodeError,
    };
    use crate::{
        BinarySymbolName, PanicAbiIdentity, ProtectedFrameAbiVersions,
        RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactId,
        RuntimeCapability, RuntimeContract, RuntimeIdentity,
        RuntimeRoleBinding, RuntimeRoleImplementation,
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
        assert!(
            RuntimeArtifact::try_new(
                metadata(),
                "bray_runtime.lib",
                RuntimeArtifactDigest::new([7; 32]),
            )
            .is_ok()
        );

        assert_eq!(
            RuntimeArtifact::try_new(
                metadata(),
                "other.lib",
                RuntimeArtifactDigest::new([7; 32]),
            ),
            Err(RuntimeArtifactBuildError::ArchiveFileNameMismatch)
        );

        assert_eq!(
            RuntimeArtifact::try_new(
                metadata(),
                "bray_runtime.lib",
                RuntimeArtifactDigest::new([8; 32]),
            ),
            Err(RuntimeArtifactBuildError::ArchiveDigestMismatch)
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

    fn metadata() -> RuntimeArtifactMetadata {
        metadata_with_identity("bray.runtime.reference")
    }

    fn metadata_with_identity(identity: &str) -> RuntimeArtifactMetadata {
        RuntimeArtifactMetadata::try_new(
            contract(identity),
            "bray_runtime.lib",
            RuntimeArtifactDigest::new([7; 32]),
        )
        .unwrap_or_else(|error| panic!("test metadata must be valid: {error:?}"))
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
                BinarySymbolName::try_new(
                    crate::MAIN_THREAD_LANE_STARTUP_SYMBOL,
                )
                .unwrap_or_else(|| panic!("runtime symbol must be valid")),
                RuntimeRoleImplementation::BrayRuntime,
            )],
        )
        .unwrap_or_else(|error| panic!("test runtime contract must be valid: {error:?}"))
    }
}
