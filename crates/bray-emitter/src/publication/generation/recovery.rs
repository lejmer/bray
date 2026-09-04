use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bray_base::{Cancellation, FileReplacementMode, sync_directory};
use serde::{Deserialize, Serialize};

use super::manifest::GenerationManifest;
use super::reference::GenerationReference;
use super::retention::{read_manifest, remove_public_file};
use super::transaction::PUBLISHED_REFERENCE;
use super::validation::{artifact_path, validate_artifact_file};
use crate::storage::{
    check_cancelled, create_managed_path, read_owned_file, stage_owned_copy, write_owned_json,
};
use crate::{StorageError, StorageErrorKind, StorageOperation};

const JOURNAL: &str = "pending-publication.json";
const REVISION: u32 = 1;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PendingPublication {
    revision: u32,
    paths: BTreeSet<String>,
}

/// The journal is durable before any public path changes. The reference remains the commit point.
pub(super) fn record_publication(
    store: &Path,
    manifest: &GenerationManifest,
) -> Result<(), StorageError> {
    write_owned_json(
        &store.join(JOURNAL),
        &PendingPublication {
            revision: REVISION,
            paths: manifest
                .artifacts
                .iter()
                .map(|artifact| artifact.published_path.to_owned())
                .collect(),
        },
    )
}

pub(super) fn complete_publication(store: &Path) -> Result<(), StorageError> {
    let path = store.join(JOURNAL);

    match std::fs::remove_file(&path) {
        Ok(()) => sync_directory(store)
            .map_err(|error| StorageError::io(store, StorageOperation::Flush, error)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StorageError::io(&path, StorageOperation::Remove, error)),
    }
}

pub(super) fn pending_publication(store: &Path) -> Result<bool, StorageError> {
    Ok(read_journal(store)?.is_some())
}

/// Requires exclusive publication ownership, or an exclusive entry lease during maintenance.
pub(super) fn recover_publication(
    root: &Path,
    store: &Path,
    cancellation: &dyn Cancellation,
) -> Result<(), StorageError> {
    let Some(journal) = read_journal(store)? else {
        return Ok(());
    };

    let reference_path = store.join(PUBLISHED_REFERENCE);

    let reference = GenerationReference::read(&reference_path)
        .map_err(|error| error.into_storage_error(&reference_path))?;

    let mut uncommitted: BTreeSet<PathBuf> = journal
        .paths
        .iter()
        .map(|path| artifact_path(root, path))
        .collect::<Result<_, _>>()?;

    if let Some(reference) = reference {
        let manifest = read_manifest(store, &reference.current)?;
        let generation = store.join(reference.current.locator);
        let staging = create_managed_path(store, Path::new("staging"))?;

        for artifact in manifest.artifacts {
            check_cancelled(store, cancellation)?;
            let source = artifact_path(&generation, &artifact.path)?;
            let destination = artifact_path(root, &artifact.published_path)?;

            validate_artifact_file(&source, &artifact, cancellation)?;

            let relative = Path::new(&artifact.published_path)
                .parent()
                .unwrap_or_else(|| Path::new(""));

            create_managed_path(root, relative)?;

            let copy = stage_owned_copy(
                &source,
                &staging,
                &destination,
                FileReplacementMode::ReplaceExisting,
                cancellation,
            )?;

            copy.promote(&destination)
                .map_err(|error| StorageError::io(&destination, StorageOperation::Rename, error))?;

            if let Some(parent) = destination.parent() {
                sync_directory(parent)
                    .map_err(|error| StorageError::io(parent, StorageOperation::Flush, error))?;
            }

            uncommitted.remove(&destination);
        }
    }

    for path in uncommitted {
        check_cancelled(store, cancellation)?;

        let relative = path
            .strip_prefix(root)
            .map_err(|_| StorageError::new(&path, StorageErrorKind::UnsafePath))?;

        remove_public_file(root, relative)?;
    }

    complete_publication(store)
}

fn read_journal(store: &Path) -> Result<Option<PendingPublication>, StorageError> {
    let path = store.join(JOURNAL);

    let bytes = match read_owned_file(&path) {
        Ok(bytes) => bytes,
        Err(error)
            if matches!(
                error.kind(),
                StorageErrorKind::Io {
                    cause: std::io::ErrorKind::NotFound,
                    ..
                }
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };

    let journal: PendingPublication =
        serde_json::from_slice(&bytes).map_err(|error| StorageError::json(&path, error))?;

    if journal.revision != REVISION {
        return Err(StorageError::new(
            &path,
            StorageErrorKind::Revision {
                expected: REVISION,
                actual: journal.revision,
            },
        ));
    }

    Ok(Some(journal))
}
