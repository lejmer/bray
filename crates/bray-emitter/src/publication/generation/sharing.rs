use std::path::Path;

use bray_base::Cancellation;

use super::manifest::{GenerationManifest, ManifestArtifact};
use super::validation::{artifact_path, validate_artifact_file};
use crate::storage::{ManagedStore, check_cancelled};
use crate::{StorageError, StorageErrorKind, StorageOperation};

pub(super) fn share_generation(
    root: &Path,
    private: &Path,
    manifest: &GenerationManifest,
    cancellation: &dyn Cancellation,
) -> Result<(), StorageError> {
    let managed = ManagedStore::open(root)?;

    for artifact in &manifest.artifacts {
        check_cancelled(private, cancellation)?;
        let key = content_key(private, manifest, artifact)?;
        let destination = artifact_path(private, &artifact.path)?;

        for candidate in managed.content_candidates(&key) {
            let source = match artifact_path(root, candidate) {
                Ok(path) => path,
                Err(error) if absent(&error) => continue,
                Err(error) => return Err(error),
            };

            match validate_artifact_file(&source, artifact, cancellation) {
                Ok(()) => {}
                Err(error) if absent(&error) => continue,
                Err(error) => return Err(error),
            }

            share_file(&source, &destination, private)?;
            break;
        }
    }

    Ok(())
}

pub(super) fn record_generation(
    root: &Path,
    directory: &Path,
    manifest: &GenerationManifest,
) -> Result<(), StorageError> {
    let mut managed = ManagedStore::open(root)?;

    for artifact in &manifest.artifacts {
        let key = content_key(directory, manifest, artifact)?;
        let path = artifact_path(directory, &artifact.path)?;

        managed.record_content(key, &path)?;
    }

    managed.save()
}

fn content_key(
    path: &Path,
    manifest: &GenerationManifest,
    artifact: &ManifestArtifact,
) -> Result<String, StorageError> {
    // Equal bytes may share storage only under the same target, toolchain, kind, and permissions.
    let key = serde_json::to_vec(&(
        &manifest.context.target,
        &manifest.context.toolchain,
        &artifact.kind,
        artifact.byte_len,
        &artifact.digest_algorithm,
        &artifact.digest,
        &artifact.permissions.logical,
        artifact.permissions.unix_mode,
        artifact.permissions.read_only,
    ))
    .map_err(|error| StorageError::json(path, error))?;

    Ok(blake3::hash(&key).to_hex().to_string())
}

fn share_file(source: &Path, destination: &Path, private: &Path) -> Result<(), StorageError> {
    let staging = tempfile::Builder::new()
        .prefix(".bray-share-")
        .tempdir_in(private)
        .map_err(|error| StorageError::io(private, StorageOperation::Create, error))?;

    let link = staging.path().join("content");

    match std::fs::hard_link(source, &link) {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::Unsupported
                    | std::io::ErrorKind::CrossesDevices
                    | std::io::ErrorKind::TooManyLinks
            ) =>
        {
            return Ok(());
        }
        Err(error) => {
            return Err(StorageError::io(
                destination,
                StorageOperation::Create,
                error,
            ));
        }
    }

    // Only the private, uncommitted copy is replaced. Writable public projections remain separate.
    std::fs::rename(&link, destination)
        .map_err(|error| StorageError::io(destination, StorageOperation::Rename, error))
}

fn absent(error: &StorageError) -> bool {
    matches!(
        error.kind(),
        StorageErrorKind::Io {
            cause: std::io::ErrorKind::NotFound,
            ..
        } | StorageErrorKind::Unavailable
    )
}
