use std::path::{Path, PathBuf};

use bray_base::{Cancellation, decode_lowercase_hex};
use bray_codegen::{ArtifactDigest, ArtifactDigestAlgorithm};

use super::manifest::{GenerationManifest, ManifestArtifact};
use super::reader::PublishedGenerationReadError;
use crate::artifact::content::validate_staged_content;
use crate::storage::{managed_directory_exists, require_file};
use crate::{ManagedArtifactPath, StorageError, StorageErrorKind};
use bray_diagnostics::DiagnosticRetainedGenerationProblem as Problem;

pub(super) fn artifact_path(root: &Path, relative: &str) -> Result<PathBuf, StorageError> {
    let relative = ManagedArtifactPath::try_new(relative)
        .ok_or_else(|| StorageError::new(&root.join(relative), StorageErrorKind::UnsafePath))?
        .to_path_buf();

    let parent = relative.parent().unwrap_or_else(|| Path::new(""));

    managed_directory_exists(root, parent)?;

    Ok(root.join(relative))
}

pub(super) fn validate_artifact_file(
    path: &Path,
    artifact: &ManifestArtifact,
    cancellation: &dyn Cancellation,
) -> Result<(), StorageError> {
    require_file(path)?;

    artifact.permissions.validate(path, artifact.kind)?;

    let algorithm = match artifact.digest_algorithm.as_str() {
        "blake3" => ArtifactDigestAlgorithm::Blake3,
        "sha256" => ArtifactDigestAlgorithm::Sha256,
        _ => {
            return Err(StorageError::new(
                path,
                StorageErrorKind::Generation(Problem::DigestAlgorithm),
            ));
        }
    };

    let digest = decode_lowercase_hex::<32>(&artifact.digest)
        .and_then(|bytes| ArtifactDigest::try_new(algorithm, bytes))
        .ok_or_else(|| {
            StorageError::new(path, StorageErrorKind::Generation(Problem::DigestEncoding))
        })?;

    validate_staged_content(path, artifact.byte_len, Some(&digest), cancellation)
        .map_err(|error| StorageError::content(path, error))?;

    Ok(())
}

pub(super) fn validate_manifest(
    path: &Path,
    manifest: &GenerationManifest,
) -> Result<(), StorageError> {
    let mut identities = std::collections::BTreeSet::new();
    let mut private_paths = std::collections::BTreeSet::new();
    let mut public_paths = std::collections::BTreeSet::new();

    for artifact in &manifest.artifacts {
        let private = ManagedArtifactPath::try_new(artifact.path.as_str());
        let public = ManagedArtifactPath::try_new(artifact.published_path.as_str());

        if private.is_none() || public.is_none() {
            return Err(StorageError::new(path, StorageErrorKind::UnsafePath));
        }

        if !identities.insert((artifact.kind, artifact.ordinal))
            || !private_paths.insert(private)
            || !public_paths.insert(public)
        {
            return Err(PublishedGenerationReadError::ArtifactUnavailable.into_storage_error(path));
        }
    }

    Ok(())
}
