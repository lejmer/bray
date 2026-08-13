use std::path::Path;

use bray_base::lowercase_hex;
use bray_codegen::{ArtifactDigest, ArtifactDigestAlgorithm, BackendArtifactKind};
use serde::{Deserialize, Serialize};

use crate::publication::operation::PreparedArtifact;
use crate::{ArtifactKind, ArtifactProducer, ArtifactRequirement, ArtifactRole};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerationManifest {
    pub(super) revision: u32,
    pub(super) product: ManifestProduct,
    pub(super) artifacts: Vec<ManifestArtifact>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestProduct {
    pub(super) package: String,
    pub(super) name: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestArtifact {
    pub(super) kind: String,
    pub(super) ordinal: u32,
    pub(super) requirement: String,
    pub(super) role: String,
    pub(super) path: String,
    pub(super) published_path: String,
    pub(super) byte_len: u64,
    pub(super) digest_algorithm: String,
    pub(super) digest: String,
    pub(super) permissions: ManifestPermissions,
    pub(super) producer: ManifestProducer,
}

impl ManifestArtifact {
    pub(super) fn new(
        artifact: &PreparedArtifact<'_, '_>,
        path: &str,
        published_path: &str,
        digest: &ArtifactDigest,
        permissions: ManifestPermissions,
    ) -> Self {
        Self {
            kind: artifact.planned.id().kind().machine_key().to_owned(),
            ordinal: artifact.planned.id().ordinal(),
            requirement: requirement_key(artifact.planned.requirement()).to_owned(),
            role: role_key(artifact.planned.role()).to_owned(),
            path: path.to_owned(),
            published_path: published_path.to_owned(),
            byte_len: artifact.content.byte_len(),
            digest_algorithm: digest_algorithm_key(digest.algorithm()).to_owned(),
            digest: lowercase_hex(digest.bytes()),
            permissions,
            producer: ManifestProducer::new(artifact.planned.producer()),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestPermissions {
    pub(super) logical: String,
    pub(super) unix_mode: Option<u32>,
}

impl ManifestPermissions {
    pub(super) fn read(path: &Path, kind: ArtifactKind) -> std::io::Result<Self> {
        let metadata = std::fs::symlink_metadata(path)?;

        if !metadata.file_type().is_file() {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
        }

        Ok(Self {
            logical: permission_key(kind).to_owned(),
            unix_mode: unix_mode(&metadata.permissions()),
        })
    }

    pub(super) fn matches(&self, path: &Path) -> std::io::Result<bool> {
        let metadata = std::fs::symlink_metadata(path)?;

        Ok(metadata.file_type().is_file() && self.unix_mode == unix_mode(&metadata.permissions()))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum ManifestProducer {
    Backend {
        name: String,
        revision: String,
        toolchain_revision: String,
        unit: String,
        artifact_kind: String,
        ordinal: u32,
    },
    PackageInterface,
    PackageImplementation,
    DependencyMetadata {
        ordinal: u32,
    },
    Linker {
        ordinal: u32,
    },
}

impl ManifestProducer {
    fn new(producer: &ArtifactProducer) -> Self {
        match producer {
            ArtifactProducer::Backend { artifact, backend } => Self::Backend {
                name: backend.name().to_owned(),
                revision: backend.revision().to_owned(),
                toolchain_revision: backend.toolchain_revision().to_owned(),
                unit: lowercase_hex(&artifact.unit().content_identity()),
                artifact_kind: backend_artifact_kind_key(artifact.kind()).to_owned(),
                ordinal: artifact.ordinal(),
            },
            ArtifactProducer::PackageInterface => Self::PackageInterface,
            ArtifactProducer::PackageImplementation => Self::PackageImplementation,
            ArtifactProducer::DependencyMetadata(identity) => Self::DependencyMetadata {
                ordinal: identity.ordinal(),
            },
            ArtifactProducer::Linker(identity) => Self::Linker {
                ordinal: identity.ordinal(),
            },
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerationReference {
    pub(super) revision: u32,
    pub(super) generation: String,
    pub(super) manifest_digest: String,
}

pub(super) const fn permission_key(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Executable | ArtifactKind::ExecutableModule => "executable",
        ArtifactKind::Assembly
        | ArtifactKind::BackendIr
        | ArtifactKind::BackendBitcode
        | ArtifactKind::RelocatableObject
        | ArtifactKind::DebugCompanion
        | ArtifactKind::PackageInterface
        | ArtifactKind::PackageImplementation
        | ArtifactKind::DependencyMetadata
        | ArtifactKind::StaticLibrary
        | ArtifactKind::SharedLibrary
        | ArtifactKind::LinkedCompanion => "data",
    }
}

const fn requirement_key(requirement: ArtifactRequirement) -> &'static str {
    match requirement {
        ArtifactRequirement::Required => "required",
        ArtifactRequirement::Optional => "optional",
    }
}

const fn role_key(role: ArtifactRole) -> &'static str {
    match role {
        ArtifactRole::Product => "product",
        ArtifactRole::Inspection => "inspection",
        ArtifactRole::LinkInput => "link_input",
        ArtifactRole::Companion => "companion",
    }
}

const fn digest_algorithm_key(algorithm: ArtifactDigestAlgorithm) -> &'static str {
    match algorithm {
        ArtifactDigestAlgorithm::Blake3 => "blake3",
        ArtifactDigestAlgorithm::Sha256 => "sha256",
    }
}

const fn backend_artifact_kind_key(kind: BackendArtifactKind) -> &'static str {
    match kind {
        BackendArtifactKind::RelocatableObject => "relocatable_object",
        BackendArtifactKind::Assembly => "assembly",
        BackendArtifactKind::BackendIr => "backend_ir",
        BackendArtifactKind::BackendBitcode => "backend_bitcode",
        BackendArtifactKind::ExecutableModule => "executable_module",
        BackendArtifactKind::DebugCompanion => "debug_companion",
    }
}

#[cfg(unix)]
fn unix_mode(permissions: &std::fs::Permissions) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;

    Some(permissions.mode() & 0o777)
}

#[cfg(not(unix))]
const fn unix_mode(_: &std::fs::Permissions) -> Option<u32> {
    None
}
