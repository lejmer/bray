use std::collections::BTreeSet;
use std::hash::Hasher;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bray_base::{StableDigestHasher, is_canonical_relative_path};
use bray_runtime_interface::RuntimeAbiVersion;
use bray_target::TargetIdentity;

use super::wire::encode_payload;

/// Fixed bundle manifest file name beneath a configured standard library root.
pub const STANDARD_LIBRARY_MANIFEST_FILE_NAME: &str = "manifest.json";
const STANDARD_LIBRARY_INTERFACE_PATH: &str = "interfaces/std.brayi";

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
    /// Compiler-owned dependency metadata.
    DependencyMetadata,
    /// Relocatable native object.
    RelocatableObject,
    /// Native static library.
    StaticLibrary,
    /// Native shared library.
    SharedLibrary,
    /// Private runtime artifact metadata.
    RuntimeArtifact,
}

impl StandardLibraryArtifactKind {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::PackageInterface => "package_interface",
            Self::DependencyMetadata => "dependency_metadata",
            Self::RelocatableObject => "relocatable_object",
            Self::StaticLibrary => "static_library",
            Self::SharedLibrary => "shared_library",
            Self::RuntimeArtifact => "runtime_artifact",
        }
    }

    pub(super) fn for_str(value: &str) -> Option<Self> {
        match value {
            "package_interface" => Some(Self::PackageInterface),
            "dependency_metadata" => Some(Self::DependencyMetadata),
            "relocatable_object" => Some(Self::RelocatableObject),
            "static_library" => Some(Self::StaticLibrary),
            "shared_library" => Some(Self::SharedLibrary),
            "runtime_artifact" => Some(Self::RuntimeArtifact),
            _ => None,
        }
    }
}

/// One immutable file selected by the standard library bundle manifest.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StandardLibraryArtifact {
    kind: StandardLibraryArtifactKind,
    path: Arc<str>,
    byte_len: u64,
    digest: StandardLibraryArtifactDigest,
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
        })
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

    /// Resolves the portable path beneath a configured bundle root.
    pub fn beneath(&self, root: &Path) -> PathBuf {
        self.path
            .split('/')
            .fold(root.to_path_buf(), |path, component| path.join(component))
    }
}

/// Exact target-native artifact set in one standard library bundle.
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

        let prefix = format!(
            "targets/{}/{}.{}/",
            target.as_str(),
            runtime_abi.major(),
            runtime_abi.minor()
        );

        if artifacts.iter().any(|artifact| {
            artifact.kind() == StandardLibraryArtifactKind::PackageInterface
                || artifact
                    .path()
                    .strip_prefix(&prefix)
                    .is_none_or(|file_name| file_name.is_empty() || file_name.contains('/'))
        }) {
            return Err(StandardLibraryManifestError::InvalidTargetArtifact);
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
}

/// Complete immutable standard library bundle manifest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardLibraryBundleManifest {
    interface: StandardLibraryArtifact,
    targets: Arc<[StandardLibraryTargetArtifacts]>,
    bundle_digest: StandardLibraryBundleDigest,
}

impl StandardLibraryBundleManifest {
    /// Creates a manifest, validates its closed inventory, and derives its bundle identity.
    pub fn try_new(
        interface: StandardLibraryArtifact,
        targets: impl IntoIterator<Item = StandardLibraryTargetArtifacts>,
    ) -> Result<Self, StandardLibraryManifestError> {
        if interface.kind() != StandardLibraryArtifactKind::PackageInterface {
            return Err(StandardLibraryManifestError::InvalidInterfaceArtifact);
        }

        if interface.path() != STANDARD_LIBRARY_INTERFACE_PATH {
            return Err(StandardLibraryManifestError::InvalidInterfaceArtifact);
        }

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

        if !paths.insert(interface.path()) {
            return Err(StandardLibraryManifestError::DuplicateArtifactPath);
        }

        for artifact in targets.iter().flat_map(|target| target.artifacts()) {
            if !paths.insert(artifact.path()) {
                return Err(StandardLibraryManifestError::DuplicateArtifactPath);
            }
        }

        let targets: Arc<[_]> = targets.into();
        let payload = encode_payload(&interface, &targets)?;
        let bundle_digest = StandardLibraryBundleDigest::for_payload(&payload)?;

        Ok(Self {
            interface,
            targets,
            bundle_digest,
        })
    }

    /// Returns the public package-interface artifact.
    pub const fn interface(&self) -> &StandardLibraryArtifact {
        &self.interface
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

/// A standard library bundle manifest violates its semantic or wire contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardLibraryManifestError {
    /// Serialized JSON is malformed or does not match the strict schema.
    Malformed,
    /// Serialized JSON is valid but not the canonical compact encoding.
    NonCanonicalEncoding,
    /// A digest is malformed.
    InvalidDigest,
    /// The public interface artifact has the wrong kind.
    InvalidInterfaceArtifact,
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
    /// The published bundle digest does not match the canonical payload.
    BundleDigestMismatch,
    /// A platform length cannot fit the manifest contract.
    LengthExceeded,
}
