use std::path::{Path, PathBuf};

use bray_base::decode_lowercase_hex;
use bray_codegen::{ArtifactDigest, ArtifactDigestAlgorithm};
use serde::Deserialize;

use super::transaction::{
    GENERATION_MANIFEST, GENERATIONS_DIRECTORY, GenerationManifest, GenerationReference,
    MANIFEST_REVISION, METADATA_DIRECTORY, PUBLISHED_REFERENCE, permission_key,
};
use crate::artifact::content::validate_staged_content;
use crate::{ArtifactKind, ManagedArtifactPath};

/// Failure to resolve one artifact from the atomically published product generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishedGenerationReadError {
    /// A generation file could not be read.
    Read(std::io::ErrorKind),
    /// The publication reference is malformed.
    MalformedReference,
    /// The publication reference uses an unsupported schema revision.
    UnsupportedReferenceRevision(u32),
    /// The referenced generation identity is not canonical.
    InvalidGenerationIdentity,
    /// The canonical manifest does not match the reference digest.
    ManifestDigestMismatch,
    /// The generation manifest is malformed.
    MalformedManifest,
    /// The generation manifest uses an unsupported schema revision.
    UnsupportedManifestRevision(u32),
    /// The generation belongs to another product.
    ProductMismatch,
    /// The requested artifact is absent or ambiguous.
    ArtifactUnavailable,
    /// The requested artifact file violates its manifest contract.
    InvalidArtifact,
}

/// Resolves and validates one artifact from the currently published product generation.
pub fn resolve_published_artifact(
    root: &Path,
    product: &bray_symbols::ProductIdentity,
    kind: ArtifactKind,
    ordinal: u32,
) -> Result<PathBuf, PublishedGenerationReadError> {
    let reference_path = root.join(METADATA_DIRECTORY).join(PUBLISHED_REFERENCE);

    let reference_bytes = std::fs::read(reference_path)
        .map_err(|error| PublishedGenerationReadError::Read(error.kind()))?;

    let reference_revision = decode_revision(&reference_bytes)
        .ok_or(PublishedGenerationReadError::MalformedReference)?;

    if reference_revision != MANIFEST_REVISION {
        return Err(PublishedGenerationReadError::UnsupportedReferenceRevision(
            reference_revision,
        ));
    }

    let reference: GenerationReference = serde_json::from_slice(&reference_bytes)
        .map_err(|_| PublishedGenerationReadError::MalformedReference)?;

    let identity = decode_lowercase_hex::<32>(&reference.generation)
        .ok_or(PublishedGenerationReadError::InvalidGenerationIdentity)?;

    let manifest_digest = decode_lowercase_hex::<32>(&reference.manifest_digest)
        .ok_or(PublishedGenerationReadError::InvalidGenerationIdentity)?;

    if identity != manifest_digest {
        return Err(PublishedGenerationReadError::InvalidGenerationIdentity);
    }

    let generation = root
        .join(METADATA_DIRECTORY)
        .join(GENERATIONS_DIRECTORY)
        .join(&reference.generation);

    let manifest_bytes = std::fs::read(generation.join(GENERATION_MANIFEST))
        .map_err(|error| PublishedGenerationReadError::Read(error.kind()))?;

    if blake3::hash(&manifest_bytes).as_bytes() != &manifest_digest {
        return Err(PublishedGenerationReadError::ManifestDigestMismatch);
    }

    let manifest_revision = decode_revision(&manifest_bytes)
        .ok_or(PublishedGenerationReadError::MalformedManifest)?;

    if manifest_revision != MANIFEST_REVISION {
        return Err(PublishedGenerationReadError::UnsupportedManifestRevision(
            manifest_revision,
        ));
    }

    let manifest: GenerationManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|_| PublishedGenerationReadError::MalformedManifest)?;

    if manifest.product.package != product.package().as_str()
        || manifest.product.name != product.name()
    {
        return Err(PublishedGenerationReadError::ProductMismatch);
    }

    let mut matching = manifest
        .artifacts
        .iter()
        .filter(|artifact| artifact.kind == kind.machine_key() && artifact.ordinal == ordinal);

    let artifact = matching
        .next()
        .ok_or(PublishedGenerationReadError::ArtifactUnavailable)?;

    if matching.next().is_some() {
        return Err(PublishedGenerationReadError::ArtifactUnavailable);
    }

    let relative = ManagedArtifactPath::try_new(artifact.path.as_str())
        .ok_or(PublishedGenerationReadError::InvalidArtifact)?;

    let path = generation.join(relative.to_path_buf());

    if artifact.permissions.logical != permission_key(kind)
        || !artifact.permissions.matches(&path).unwrap_or(false)
    {
        return Err(PublishedGenerationReadError::InvalidArtifact);
    }

    let algorithm = match artifact.digest_algorithm.as_str() {
        "blake3" => ArtifactDigestAlgorithm::Blake3,
        "sha256" => ArtifactDigestAlgorithm::Sha256,
        _ => return Err(PublishedGenerationReadError::InvalidArtifact),
    };

    let digest_bytes = decode_lowercase_hex::<32>(&artifact.digest)
        .ok_or(PublishedGenerationReadError::InvalidArtifact)?;

    let digest = ArtifactDigest::try_new(algorithm, digest_bytes)
        .ok_or(PublishedGenerationReadError::InvalidArtifact)?;

    validate_staged_content(&path, artifact.byte_len, Some(&digest), &|| false)
        .map_err(|_| PublishedGenerationReadError::InvalidArtifact)?;

    Ok(path)
}

#[derive(Deserialize)]
struct RevisionProbe {
    revision: u32,
}

fn decode_revision(bytes: &[u8]) -> Option<u32> {
    serde_json::from_slice::<RevisionProbe>(bytes)
        .ok()
        .map(|probe| probe.revision)
}
