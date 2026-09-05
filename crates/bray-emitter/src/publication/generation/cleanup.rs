use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bray_base::sync_directory;

use super::manifest::GenerationManifest;
use super::reference::{GenerationReference, GenerationReferenceEntry};
use super::transaction::{ManagedLayout, storage_failure};
use crate::publication::diagnostic::{PublicationError, PublicationErrorKind};
use crate::publication::operation::{ArtifactPublicationFailure, artifact_failure, planned_error};

pub(super) fn generation_public_paths(
    root: &Path,
    layout: &ManagedLayout,
    reference: &GenerationReferenceEntry,
    planned: &crate::PlannedArtifact,
) -> Result<BTreeSet<PathBuf>, ArtifactPublicationFailure> {
    let manifest = super::retention::read_manifest(&layout.metadata, reference)
        .map_err(|error| storage_failure(planned, error))?;

    manifest
        .artifacts
        .into_iter()
        .map(|artifact| {
            crate::ManagedArtifactPath::try_new(artifact.published_path)
                .map(|path| root.join(path.to_path_buf()))
                .ok_or_else(|| {
                    artifact_failure(planned, PublicationErrorKind::InvalidGenerationManifest)
                })
        })
        .collect()
}

pub(super) fn stale_public_paths(
    root: &Path,
    manifest: &GenerationManifest,
    mut preceding: BTreeSet<PathBuf>,
    planned: &crate::PlannedArtifact,
) -> Result<BTreeSet<PathBuf>, ArtifactPublicationFailure> {
    for artifact in &manifest.artifacts {
        let path = crate::ManagedArtifactPath::try_new(artifact.published_path.as_str())
            .ok_or_else(|| {
                artifact_failure(planned, PublicationErrorKind::InvalidGenerationManifest)
            })?;

        preceding.remove(&root.join(path.to_path_buf()));
    }

    Ok(preceding)
}

pub(super) fn retain_recent_generations(
    root: &Path,
    layout: &ManagedLayout,
    reference: &GenerationReference,
    planned: &crate::PlannedArtifact,
    cancellation: &dyn bray_base::Cancellation,
) -> Result<(), PublicationError> {
    let mut managed = crate::storage::ManagedStore::open(root)
        .map_err(|error| planned_error(planned, PublicationErrorKind::Storage(Box::new(error))))?;

    super::retention::prune_generations(
        &mut managed,
        &layout.metadata,
        Some(reference),
        cancellation,
    )
    .map_err(|error| planned_error(planned, PublicationErrorKind::Storage(Box::new(error))))?;

    sync_directory(&layout.metadata).map_err(|error| {
        planned_error(
            planned,
            PublicationErrorKind::Storage(Box::new(crate::StorageError::io(
                &layout.metadata,
                crate::StorageOperation::Flush,
                error,
            ))),
        )
    })
}
