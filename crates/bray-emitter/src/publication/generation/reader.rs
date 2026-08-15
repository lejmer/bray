use std::path::PathBuf;

use bray_base::decode_lowercase_hex;
use bray_codegen::{ArtifactDigest, ArtifactDigestAlgorithm};
use serde::Deserialize;

use super::layout::product_store;
use super::lock::open_lock_file;
use super::manifest::{GenerationManifest, GenerationReference, permission_key};
use super::transaction::{
    GENERATION_MANIFEST, GENERATIONS_DIRECTORY, MANIFEST_REVISION, PUBLISHED_REFERENCE,
};
use crate::artifact::content::validate_staged_content;
use crate::{
    ArtifactKind, ManagedArtifactPath, ManagedFilesystemDestination, ManagedOutputDirectory,
};

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

/// Shared product-publication lock held while stable public artifact paths are consumed.
pub struct PublishedProductReadGuard {
    _file: std::fs::File,
}

/// Waits until any product publication finishes and prevents replacement until this guard drops.
pub fn lock_published_product(
    destination: impl Into<ManagedFilesystemDestination>,
    product: &bray_symbols::ProductIdentity,
) -> Result<PublishedProductReadGuard, PublishedGenerationReadError> {
    let destination = destination.into();

    lock_product_destination(&destination, product)
}

fn lock_product_destination(
    destination: &ManagedFilesystemDestination,
    product: &bray_symbols::ProductIdentity,
) -> Result<PublishedProductReadGuard, PublishedGenerationReadError> {
    let public_directory = destination
        .relative_directory()
        .map_or_else(PathBuf::new, ManagedOutputDirectory::to_path_buf);

    let store = product_store(destination.root(), &public_directory, product);

    let file =
        open_lock_file(&store).map_err(|error| PublishedGenerationReadError::Read(error.kind()))?;

    file.lock_shared()
        .map_err(|error| PublishedGenerationReadError::Read(error.kind()))?;

    Ok(PublishedProductReadGuard { _file: file })
}

/// Resolves and validates one artifact from the currently published product generation.
pub fn resolve_published_artifact(
    destination: impl Into<ManagedFilesystemDestination>,
    product: &bray_symbols::ProductIdentity,
    kind: ArtifactKind,
    ordinal: u32,
) -> Result<PathBuf, PublishedGenerationReadError> {
    let destination = destination.into();

    let public_directory = destination
        .relative_directory()
        .map_or_else(PathBuf::new, ManagedOutputDirectory::to_path_buf);

    let store = product_store(destination.root(), &public_directory, product);
    let _guard = lock_product_destination(&destination, product)?;

    let reference_path = store.join(PUBLISHED_REFERENCE);

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

    let generation = store
        .join(GENERATIONS_DIRECTORY)
        .join(&reference.generation);

    let manifest_bytes = std::fs::read(generation.join(GENERATION_MANIFEST))
        .map_err(|error| PublishedGenerationReadError::Read(error.kind()))?;

    if blake3::hash(&manifest_bytes).as_bytes() != &manifest_digest {
        return Err(PublishedGenerationReadError::ManifestDigestMismatch);
    }

    let manifest_revision =
        decode_revision(&manifest_bytes).ok_or(PublishedGenerationReadError::MalformedManifest)?;

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

    let internal_path = generation.join(relative.to_path_buf());

    let published = ManagedArtifactPath::try_new(artifact.published_path.as_str())
        .ok_or(PublishedGenerationReadError::InvalidArtifact)?;

    let path = destination.root().join(published.to_path_buf());

    if artifact.permissions.logical != permission_key(kind)
        || !artifact
            .permissions
            .matches(&internal_path)
            .unwrap_or(false)
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

    validate_staged_content(&internal_path, artifact.byte_len, Some(&digest), &|| false)
        .map_err(|_| PublishedGenerationReadError::InvalidArtifact)?;

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
